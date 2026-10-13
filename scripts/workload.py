#!/usr/bin/env python3
"""Deterministic synthetic public-CLI workload; no latency pass/fail thresholds.

Examples (both use exactly the same versioned corpus):
  python3 scripts/workload.py --binary dist/m6-macos/copperline-cli --profile small
  python3 scripts/workload.py --binary target/release/copperline-cli --profile medium

The binary is always copied into a private temporary directory before chmod/run.
Only a new workspace and a new evidence directory are accepted. Defaults retain
both under a newly allocated temporary directory; the final JSON prints paths.
Measurements include local JSON-lines IPC, not Python request encoding/decoding.
Warm reopen includes process startup, full-history validation and first state
response. It is not a cold-cache or authenticated multi-user benchmark.
"""

import argparse
import csv
import hashlib
import io
import json
import math
import os
from pathlib import Path
import platform
import queue
import shutil
import statistics
import subprocess
import sys
import tempfile
import threading
import time
import traceback


SCHEMA = "copperline-workload-v1"
MAX_HISTORY = 512 * 1024 * 1024
MAX_SNAPSHOT = 16 * 1024 * 1024
PROFILES = {
    "small": dict(products=10, customers=3, invoices=100, batch=25, quotes=5,
                  history_updates=20, evaluate_samples=50, retry_samples=20, reopen_samples=3),
    "medium": dict(products=50, customers=10, invoices=1000, batch=100, quotes=20,
                   history_updates=100, evaluate_samples=100, retry_samples=50, reopen_samples=5),
}


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def encoded(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def digest(value):
    return hashlib.sha256(encoded(value)).hexdigest()


def file_digest(path):
    h = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def write_json_new(path, value):
    with path.open("x", encoding="utf-8", newline="\n") as destination:
        json.dump(value, destination, ensure_ascii=False, indent=2, sort_keys=True)
        destination.write("\n")


def rounded(value, numerator, denominator):
    magnitude, remainder = divmod(abs(value) * numerator, denominator)
    magnitude += int(2 * remainder >= denominator)
    return magnitude if value >= 0 else -magnitude


def decimal(cents):
    return f"{cents // 100}.{cents % 100:02d}"


def product_values(index):
    return 10_000 + 137 * index, 5_000 + 61 * index


def price(product, converted):
    result = dict(product_id=f"P-{product:03d}", customer_id="C-000", date="2026-10-13",
                  quantity=1000 if converted else 10_000, discount=100, rebate=200,
                  source={"kind": "customer_contract"})
    if converted:
        result["conversion"] = dict(currency="EUR", unit="box")
    return result


def expected_price(request):
    index = int(request["product_id"].split("-")[1])
    listed, cost = product_values(index)
    ruled = rounded(listed, 9, 10)
    converted = "conversion" in request
    if converted:
        listed, ruled, cost = (rounded(v, 108, 10) for v in (listed, ruled, cost))
    invoice_unit = ruled - rounded(ruled, request["discount"], 10_000)
    invoice_total = rounded(invoice_unit, request["quantity"], 1000)
    rebate_total = rounded(invoice_total, request["rebate"], 10_000)
    pocket_total = invoice_total - rebate_total
    cost_total = rounded(cost, request["quantity"], 1000)
    return dict(currency="EUR" if converted else "USD", unit="box" if converted else "each",
                list_unit=listed, rule_unit=ruled, invoice_unit=invoice_unit, cost_unit=cost,
                invoice_total=invoice_total, rebate_total=rebate_total, pocket_total=pocket_total,
                cost_total=cost_total, margin_total=pocket_total - cost_total)


def check_price(actual, request):
    for field, expected in expected_price(request).items():
        require(actual[field] == expected, f"Financial mismatch {field}: {actual[field]} != {expected}")
    require(actual["source"]["source"] == {"kind": "contract", "id": "CONTRACT", "edition": 1},
            "Quote/evaluation did not pin the approved customer contract")
    if "conversion" in request:
        require(actual["conversion"]["native_quantity"] == request["quantity"] * 12,
                "UOM qualification must use native quantity")
        require(actual["conversion"]["fx"]["entry"]["ratio"] == {"numerator": 9, "denominator": 10},
                "FX evidence differs from declared direct rate")


def build_corpus(profile):
    spec = PROFILES[profile]
    commands = []

    def add(command, params, phase, role="analyst", name="Synthetic analyst"):
        revision = len(commands)
        request = dict(request_id=f"workload-{revision + 1:05d}", expected_revision=revision,
                       actor=dict(name=name, role=role), command=command, params=params)
        commands.append(dict(phase=phase, request=request))

    for i in range(spec["products"]):
        listed, cost = product_values(i)
        add("product_put", dict(id=f"P-{i:03d}", name=f"Synthetic industrial component {i:03d}",
                               family=f"Family-{i % 5}", unit="each", currency="USD", list_price=listed), "catalog")
        add("cost_add", dict(product_id=f"P-{i:03d}", version=1, effective_from="2026-01-01",
                            amount=cost, currency="USD", unit="each"), "catalog")
    for i in range(spec["customers"]):
        add("customer_put", dict(id=f"C-{i:03d}", name=f"Synthetic customer {i:03d}", segment="Industrial"), "catalog")
    entries = [dict(id="FX-USD-EUR", conversion=dict(kind="fx", **{"from": "USD", "to": "EUR"}),
                    start="2026-01-01", end="2027-01-01", ratio=dict(numerator=9, denominator=10))]
    entries += [dict(id=f"UOM-{i:03d}", conversion=dict(kind="uom", product_id=f"P-{i:03d}", from_unit="box", to_unit="each"),
                     start="2026-01-01", end="2027-01-01", ratio=dict(numerator=12, denominator=1))
                for i in range(spec["products"])]
    add("conversion_create", dict(terms=dict(id="BOOK", name="Synthetic direct conversion book", entries=entries)), "conversion")
    add("conversion_submit", dict(id="BOOK", version=1), "conversion")
    add("conversion_decide", dict(id="BOOK", version=2, approve=True, note="Independent synthetic rate review"),
        "conversion", "approver", "Synthetic rate reviewer")
    rows = [dict(id=f"ROW-{i:03d}", product_id=f"P-{i:03d}", unit="each", min_quantity=1000,
                 max_quantity=None, basis=dict(kind="fixed", invoice_unit=rounded(product_values(i)[0], 9, 10)))
            for i in range(spec["products"])]
    terms = dict(id="CONTRACT", edition=1, name="Synthetic native customer contract", customer_id="C-000",
                 currency="USD", start="2026-01-01", end="2027-01-01", priority=10, rows=rows)
    add("contract_create", dict(terms=terms), "contract")
    add("contract_submit", dict(id="CONTRACT", edition=1, version=1), "contract")
    add("contract_decide", dict(id="CONTRACT", edition=1, version=2, approve=True, note="Independent synthetic contract review"),
        "contract", "approver", "Synthetic contract reviewer")
    invoice_totals = {}
    for first in range(0, spec["invoices"], spec["batch"]):
        stream = io.StringIO(newline="")
        writer = csv.writer(stream, lineterminator="\n")
        writer.writerow("id source product_id customer_id date quantity unit currency invoice_total cost_total".split())
        for i in range(first, min(first + spec["batch"], spec["invoices"])):
            p = i % spec["products"]
            customer = f"C-{i % spec['customers']:03d}"
            listed, cost = product_values(p)
            quantity = 1 + i % 9
            invoice = rounded(listed, 9, 10) * quantity
            cost *= quantity
            writer.writerow([f"INV-{i:05d}", "synthetic-workload", f"P-{p:03d}", customer,
                             f"2026-10-{1 + i % 28:02d}", quantity, "each", "USD", decimal(invoice), decimal(cost)])
            group = invoice_totals.setdefault(customer, dict(invoice=0, cost=0, converted_invoice=0, converted_cost=0))
            group["invoice"] += invoice
            group["cost"] += cost
            group["converted_invoice"] += rounded(invoice, 9, 10)
            group["converted_cost"] += rounded(cost, 9, 10)
        add("transactions_import", dict(csv=stream.getvalue()), "invoice_import")
    quotes = {}
    for i in range(spec["quotes"]):
        qid = f"Q-{i:03d}"
        converted = i % 2 == 1
        requests = [price((2 * i + line) % spec["products"], converted) for line in range(2)]
        quotes[qid] = requests
        add("quote_create", dict(id=qid, title=f"Synthetic {'EUR box' if converted else 'USD each'} quote {i:03d}",
                                 customer_id="C-000", date="2026-10-13"), "quote", "sales", "Synthetic seller")
        for line, request in enumerate(requests):
            add("quote_add_line", dict(id=qid, version=line + 1, request=request), "quote", "sales", "Synthetic seller")
        add("quote_submit", dict(id=qid, version=3), "quote", "sales", "Synthetic seller")
        add("quote_decide", dict(id=qid, version=4, approve=True, note="Independent synthetic quote review"),
            "quote", "approver", "Synthetic quote reviewer")
    # Same bounded live catalog, longer append-only history: a separate dimension.
    for i in range(spec["history_updates"]):
        customer = i % spec["customers"]
        add("customer_put", dict(id=f"C-{customer:03d}", name=f"Synthetic customer {customer:03d} revision {i:03d}",
                                 segment="Industrial"), "history_updates")
    reads = [price(i % spec["products"], bool(i % 2)) for i in range(spec["evaluate_samples"])]
    retry_indexes = [i * (len(commands) - 1) // max(1, spec["retry_samples"] - 1) for i in range(spec["retry_samples"])]
    return dict(schema=SCHEMA, profile=profile, specification=spec, commands=commands, evaluations=reads,
                retry_command_indexes=retry_indexes, expected_quotes=quotes, expected_invoice_totals=invoice_totals)


class Api:
    def __init__(self, binary, directory, evidence, serial, timeout):
        self.timeout = timeout
        self.stderr_path = evidence / f"process-{serial:02d}.stderr.txt"
        self.stderr = self.stderr_path.open("xb")
        self.process = subprocess.Popen([str(binary), "--data", str(directory), "api"],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr)
        self.responses = queue.Queue()

        def read():
            try:
                for line in self.process.stdout:
                    self.responses.put(line)
            finally:
                self.responses.put(None)

        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()

    def call(self, method, params=None, okay=True):
        body = encoded(dict(method=method, params=params)) + b"\n"
        require(len(body) <= 2 * 1024 * 1024, "Workload request exceeds public transport limit")
        start = time.perf_counter_ns()
        self.process.stdin.write(body)
        self.process.stdin.flush()
        try:
            line = self.responses.get(timeout=self.timeout)
        except queue.Empty as exc:
            raise RuntimeError(f"Timed out waiting for {method}; inspect {self.stderr_path}") from exc
        self.received_ns = time.perf_counter_ns()
        elapsed = (self.received_ns - start) / 1_000_000
        require(line is not None, f"CLI exited during {method}; inspect {self.stderr_path}")
        response = json.loads(line)
        require(response.get("ok") is okay, f"{method}: unexpected response {response}")
        return response.get("result") if okay else response, elapsed

    def close(self):
        if self.process.poll() is None:
            self.process.stdin.close()
            try:
                self.process.wait(timeout=min(self.timeout, 30))
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=10)
        self.reader.join(timeout=2)
        self.process.stdout.close()
        self.stderr.close()
        require(self.process.returncode == 0, f"CLI exit {self.process.returncode}; inspect {self.stderr_path}")

    def abort(self):
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait(timeout=10)
        self.stderr.close()


def history_files(directory):
    files = sorted(p for p in directory.glob("*.json") if p.stem.isdigit() and len(p.stem) == 20)
    return files, sum(p.stat().st_size for p in files)


def summary(values):
    ordered = sorted(values)
    require(bool(ordered), "Cannot summarize an empty timing sample")
    return dict(count=len(ordered), min_ms=min(ordered), p50_ms=statistics.median(ordered),
                p95_ms=ordered[math.ceil(0.95 * len(ordered)) - 1], max_ms=max(ordered))


def check_state(state, corpus):
    spec = corpus["specification"]
    expected_revision = len(corpus["commands"])
    require(state["revision"] == expected_revision, "Unexpected final revision")
    for key, count in [("products", spec["products"]), ("customers", spec["customers"]),
                       ("costs", spec["products"]), ("transactions", spec["invoices"]), ("quotes", spec["quotes"])]:
        require(len(state[key]) == count, f"Unexpected {key} count")
    require(len(state["audit"]) == len(state["receipts"]) == expected_revision, "Audit/receipt count mismatch")
    for i, (command, receipt, audit) in enumerate(zip(corpus["commands"], state["receipts"], state["audit"]), 1):
        require(receipt["request_id"] == command["request"]["request_id"] and receipt["revision"] == i,
                "Receipt identity/revision mismatch")
        require(audit["revision"] == i and audit["command"] == command["request"]["command"], "Audit sequence mismatch")
        require(len(receipt["fingerprint"]) == 64, "Receipt fingerprint missing")
    for qid, requests in corpus["expected_quotes"].items():
        quote = state["quotes"][qid]
        require(quote["status"] == "approved" and len(quote["lines"]) == 2, "Quote approval/line count mismatch")
        for line, request in zip(quote["lines"], requests):
            check_price(line, request)


def collect_exports(api, corpus, evidence=None):
    exports = {}
    for qid, requests in corpus["expected_quotes"].items():
        content, _ = api.call("export_quote", dict(id=qid))
        raw = content["csv"].encode("utf-8")
        rows = list(csv.DictReader(io.StringIO(content["csv"])))
        require(len(rows) == 2, "Quote CSV lost a line")
        for row, request in zip(rows, requests):
            expected = expected_price(request)
            require(row["currency"] == expected["currency"] and row["invoice_total"] == decimal(expected["invoice_total"]),
                    "Quote export disagrees with exact expected amount")
        exports[qid] = hashlib.sha256(raw).hexdigest()
        if evidence is not None:
            with (evidence / f"{qid}.csv").open("xb") as target:
                target.write(raw)
    return exports


def backup_roundtrip(binary, directory, evidence, state, exports, corpus, manifest, timeout):
    """Opt-in M7 commands; never invoked by the default/M6 corpus."""
    archive = evidence / "workspace.clbak"
    restored = evidence / "restored-workspace"
    require(not archive.exists() and not restored.exists(), "Backup/restore destinations must be new")
    results = {}
    for name, arguments in [
        ("backup", ["--data", str(directory), "backup", str(archive)]),
        ("backup_inspect", ["backup-inspect", str(archive)]),
        ("restore", ["restore", str(archive), str(restored)]),
    ]:
        start = time.perf_counter_ns()
        completed = subprocess.run([str(binary), *arguments], capture_output=True, timeout=timeout)
        elapsed = (time.perf_counter_ns() - start) / 1_000_000
        require(completed.returncode == 0, f"Optional {name} failed: {completed.stderr.decode('utf-8', errors='replace')}")
        result = json.loads(completed.stdout)
        require(isinstance(result, dict), f"Optional {name} must return a JSON object")
        results[name] = dict(timing=summary([elapsed]), output=result)
    restored_files, restored_bytes = history_files(restored)
    restored_manifest = [dict(name=p.name, bytes=p.stat().st_size, sha256=file_digest(p)) for p in restored_files]
    require(restored_manifest == manifest, "Restore changed exact committed snapshot bytes")
    api = Api(binary, restored, evidence, 100, timeout)
    try:
        restored_state, _ = api.call("state")
        require(restored_state == state, "Restore changed validated business state")
        require(collect_exports(api, corpus) == exports, "Restore changed approved exports")
        receipt, _ = api.call("execute", corpus["commands"][0]["request"])
        require(receipt["replayed"] and receipt["revision"] == 1, "Restore lost the original idempotency receipt")
        api.close()
    except Exception:
        api.abort()
        raise
    results.update(archive=str(archive), archive_bytes=archive.stat().st_size, archive_sha256=file_digest(archive),
                   restored_directory=str(restored), restored_history_bytes=restored_bytes,
                   verified=["exact committed file names/lengths/hashes", "full validated state", "approved export bytes", "original receipt replay"])
    return results


def compare_baseline(report, path):
    baseline = json.loads(path.read_text(encoding="utf-8"))
    require(baseline.get("result") == "PASS" and baseline.get("schema") == SCHEMA, "Baseline result is not a passing compatible workload")
    require(baseline["corpus_sha256"] == report["corpus_sha256"], "Baseline uses a different command corpus")
    for field in ["state_sha256", "export_sha256", "receipts"]:
        require(baseline["correctness"][field] == report["correctness"][field], f"Baseline compatibility mismatch: {field}")
    ratios = {}
    for operation in ["durable_command", "durable_command/history_updates", "evaluate", "exact_retry", "warm_full_history_reopen"]:
        ratios[operation] = {
            metric + "_ratio_to_baseline": report["latencies"][operation][metric] / baseline["latencies"][operation][metric]
            for metric in ["p50_ms", "p95_ms"]
        }
    return dict(baseline_result=str(path), baseline_result_sha256=file_digest(path),
                baseline_binary_sha256=baseline["environment"]["binary_sha256"], latency_ratios=ratios,
                identical_corpus_state_exports_receipts=True,
                identical_history_files=baseline["storage"]["history_manifest_sha256"] == report["storage"]["history_manifest_sha256"],
                caution="One local workload sample per binary; ratios are descriptive, not causal evidence or pass thresholds")


def run(args, directory, evidence, report):
    source_binary = Path(args.binary).resolve(strict=True)
    private = Path(tempfile.mkdtemp(prefix="copperline-workload-binary-", dir="/tmp" if Path("/tmp").is_dir() else None))
    binary = private / ("copperline-cli.exe" if os.name == "nt" else "copperline-cli")
    shutil.copyfile(source_binary, binary)
    binary.chmod(0o700)  # Never change the supplied release artifact's permissions.
    binary_hash = file_digest(binary)
    require(binary_hash == file_digest(source_binary), "Private binary copy differs")
    version = subprocess.run([str(binary), "--version"], capture_output=True, text=True, encoding="utf-8", timeout=30, check=True)
    report["environment"] = dict(platform=platform.platform(), system=platform.system(), machine=platform.machine(),
                                 python=sys.version, processor=platform.processor(), process_id=os.getpid(),
                                 binary_source=str(source_binary), binary_private_copy=str(binary), binary_sha256=binary_hash,
                                 binary_bytes=binary.stat().st_size, binary_version=version.stdout.strip(),
                                 script_sha256=file_digest(Path(__file__).resolve()))
    corpus = build_corpus(args.profile)
    report["corpus_sha256"] = digest(corpus)
    report["specification"] = corpus["specification"]
    write_json_new(evidence / "corpus.json", corpus)
    timings = []
    api = Api(binary, directory, evidence, 0, args.timeout)
    try:
        initial, _ = api.call("state")
        require(initial["revision"] == 0 and not initial["products"], "Workload requires a new empty workspace")
        previous = 0
        for item in corpus["commands"]:
            _, history_bytes = history_files(directory)
            require(history_bytes + MAX_SNAPSHOT <= MAX_HISTORY, "Stopped before the next command could exceed the 512 MiB history bound")
            request = item["request"]
            result, elapsed = api.call("execute", request)
            require(not result["replayed"] and result["revision"] == previous + 1, "Durable command revision/replay mismatch")
            timings.append(dict(operation="durable_command", phase=item["phase"], command=request["command"],
                                revision_before=previous, revision_after=result["revision"], elapsed_ms=elapsed))
            previous = result["revision"]
        # These are explicit untimed warm-up requests; sample counts remain honest.
        for r in corpus["evaluations"][:5]:
            result, _ = api.call("evaluate", r)
            check_price(result, r)
        for r in corpus["evaluations"]:
            result, elapsed = api.call("evaluate", r)
            check_price(result, r)
            timings.append(dict(operation="evaluate", phase="converted" if "conversion" in r else "native", elapsed_ms=elapsed))
        for index in corpus["retry_command_indexes"]:
            request = corpus["commands"][index]["request"]
            result, elapsed = api.call("execute", request)
            require(result["replayed"] and result["revision"] == index + 1, "Exact retry failed to recover original receipt")
            timings.append(dict(operation="exact_retry", command=request["command"], elapsed_ms=elapsed))
        state, _ = api.call("state")
        check_state(state, corpus)
        # Same receipt key with changed contents must fail without a revision/state change.
        changed = json.loads(json.dumps(corpus["commands"][0]["request"]))
        changed["params"]["name"] = "Synthetic forbidden receipt-key reuse"
        api.call("execute", changed, okay=False)
        unchanged, _ = api.call("state")
        require(unchanged == state, "Retry or rejected request changed state")
        actuals, _ = api.call("converted_actuals", dict(start="2026-10-01", end="2026-11-01", currency="EUR"))
        require(len(actuals["rows"]) == corpus["specification"]["invoices"], "Reporting omitted invoices")
        for group in actuals["groups"]:
            expected = corpus["expected_invoice_totals"][group["customer_id"]]
            require(group["invoice"] == expected["converted_invoice"] and group["cost"] == expected["converted_cost"],
                    "Reporting arithmetic differs from independently summed converted invoice totals")
        exports = collect_exports(api, corpus, evidence)
        api.close()
        api = None
        for i in range(corpus["specification"]["reopen_samples"]):
            start = time.perf_counter_ns()
            api = Api(binary, directory, evidence, i + 1, args.timeout)
            reopened, _ = api.call("state")
            elapsed = (api.received_ns - start) / 1_000_000
            require(reopened == state, "Full-history reopen changed state")
            timings.append(dict(operation="warm_full_history_reopen", elapsed_ms=elapsed))
            if i == 0:
                require(collect_exports(api, corpus) == exports, "Frozen exports changed on reopen")
                first_request = corpus["commands"][0]["request"]
                receipt, _ = api.call("execute", first_request)
                require(receipt["replayed"] and receipt["revision"] == 1, "Original receipt was lost on reopen")
                still_same, _ = api.call("state")
                require(still_same == state, "Reopened retry mutated state")
            api.close()
            api = None
        files, history_bytes = history_files(directory)
        require(len(files) == state["revision"] + 1, "Missing committed revision including genesis")
        require(history_bytes <= MAX_HISTORY, "History byte cap exceeded")
        groups = {}
        for row in timings:
            groups.setdefault(row["operation"], []).append(row["elapsed_ms"])
            if "phase" in row:
                groups.setdefault(row["operation"] + "/" + row["phase"], []).append(row["elapsed_ms"])
            if row["operation"] == "durable_command":
                groups.setdefault("command/" + row["command"], []).append(row["elapsed_ms"])
        report["latencies"] = {key: summary(values) for key, values in sorted(groups.items())}
        manifest = [dict(name=p.name, bytes=p.stat().st_size, sha256=file_digest(p)) for p in files]
        report["storage"] = dict(committed_revision=state["revision"], committed_files=len(files),
                                 durable_commands=len(corpus["commands"]), state_json_bytes=len(encoded(state)),
                                 state_json_encoding="canonical compact sorted-key UTF-8", latest_snapshot_bytes=files[-1].stat().st_size,
                                 total_history_bytes=history_bytes, history_limit_bytes=MAX_HISTORY,
                                 history_manifest_sha256=digest(manifest))
        report["correctness"] = dict(state_sha256=digest(state), export_sha256=exports, receipts=len(state["receipts"]),
                                     checks=["exact fixed-point quote and evaluation arithmetic", "approved native contract and frozen conversion evidence",
                                             "batched invoice totals and converted reporting", "audit/receipt/revision identity", "original receipt retries before/after reopen",
                                             "changed-content receipt rejection preserves state", "full state equality after each reopen", "frozen export byte equality after reopen"])
        write_json_new(evidence / "state.json", state)
        write_json_new(evidence / "timings.json", timings)
        write_json_new(evidence / "history-manifest.json", manifest)
        if args.backup:
            report["backup_roundtrip"] = backup_roundtrip(binary, directory, evidence, state, exports, corpus, manifest, args.timeout)
        if args.baseline_result is not None:
            report["baseline_comparison"] = compare_baseline(report, args.baseline_result.resolve(strict=True))
        report["result"] = "PASS"
    finally:
        if api is not None:
            api.abort()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", required=True, help="CLI binary; copied privately before execution")
    parser.add_argument("--profile", choices=PROFILES, default="small")
    parser.add_argument("--output-data", type=Path, help="New-only durable workspace directory (default: retained temp workspace)")
    parser.add_argument("--evidence-dir", type=Path, help="New-only evidence directory (default: sibling of workspace)")
    parser.add_argument("--timeout", type=float, default=60, help="Per-response safety timeout, seconds; not a timing pass threshold")
    parser.add_argument("--backup", action="store_true", help="Opt into M7 backup/backup-inspect/restore timing and exact restoration checks; unsupported by M6")
    parser.add_argument("--baseline-result", type=Path, help="Compare a prior result.json: require identical corpus/state/exports/receipts, report descriptive latency ratios")
    args = parser.parse_args()
    require(args.timeout > 0, "Timeout must be positive")
    root = Path(tempfile.mkdtemp(prefix="copperline-workload-")) if args.output_data is None else None
    directory = (args.output_data or root / "data").absolute()
    evidence = (args.evidence_dir or directory.with_name(directory.name + "-evidence")).absolute()
    require(not directory.exists() and not directory.is_symlink(), f"Refusing existing data path: {directory}")
    require(not evidence.exists() and not evidence.is_symlink(), f"Refusing existing evidence path: {evidence}")
    require(directory != evidence and directory not in evidence.parents and evidence not in directory.parents,
            "Workspace and evidence directories must be separate, non-nested paths")
    directory.mkdir(parents=True, exist_ok=False)
    evidence.mkdir(parents=True, exist_ok=False)
    report = dict(schema=SCHEMA, profile=args.profile, data_directory=str(directory), evidence_directory=str(evidence),
                  measurement_notes=["Synthetic deterministic corpus; no company data, clocks or randomness in business commands",
                                     "API times include JSON-lines IPC and durable fsync where applicable; exclude Python encoding/decoding",
                                     "p95 is nearest rank; p50 is the sample median; no performance thresholds",
                                     "Reopen uses warm OS caches, process startup, full-history verification and first state JSON; no cache flushing",
                                     "Single local CLI writer; concurrent-user throughput and cold storage are not measured"])
    try:
        run(args, directory, evidence, report)
    except Exception as exc:
        report.update(result="FAIL", error=str(exc), traceback=traceback.format_exc())
        write_json_new(evidence / "result.json", report)
        print(json.dumps(report, indent=2, sort_keys=True))
        return 1
    write_json_new(evidence / "result.json", report)
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
