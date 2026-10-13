# Repository review — 2026-10-13

Reviewed the public [BLTCPT/copperline](https://github.com/BLTCPT/copperline) repository at `64dd0736ae2dc06901528bcce6c9dfd0715dc277` (local HEAD and origin/main matched), plus the notice repairs described below. This independent review found no credential-pattern match, private company fixture or unsupported full-parity claim in the inspected material. Dependency notice gaps required repairs; one upstream attribution gap and an upstream licensing caveat remain explicit.

## Scope and method

- Enumerated all locally reachable public history with `git rev-list --all` and `git rev-list --objects --all`; read object contents through `git cat-file --batch`. Scope: **22 commits, 465 unique blobs, 441 text blobs, 22,487,286 bytes**, and 193 currently tracked files. No private user directories, ignored workspaces, local data, credentials, reflogs or unreachable objects were scanned. Later commits are outside this snapshot.
- Standard-library byte-pattern checks covered private-key headers, common GitHub/AWS/Slack/Google/OpenAI token formats, JWT-shaped strings, credential URLs, quoted secret assignments, credential-like filenames, company/confidentiality terms and local absolute paths. Matches were reported by category/path only; potential credential values were not printed. Source, fixtures, README, AGENTS, attribution, security/parity/gaps documents, packaging and build guidance were reviewed separately. This is a bounded pattern/manual audit, not a guarantee that arbitrary secrets or private content cannot exist; screenshots were not exhaustively OCR-scanned.
- Compared every external lockfile name/version with the TSV inventory and generated notice sections: **313/313 external packages covered**, plus nine original workspace packages in the 322-row inventory. Reviewed notice content separately from that metadata coverage. No vulnerability database scan, legal clearance, authenticated enterprise security assessment or new dependency installation was performed.

## Findings and disposition

| Finding | Evidence and disposition |
|---|---|
| Credentials/private business material | No credential-pattern candidates or credential-like tracked filenames. Company/proprietary terms found in authored text were clean-room disclaimers; dependency legal text accounted for other matches. Synthetic fixtures and caller-selected identities are explicit. Automated pattern coverage is not proof of absence. |
| Historical developer path | `evidence/quality-gates.txt` at `0efb5b627bdbafdb1f5e7a97f0a0b36377eec66c` contains ten ordinary local workspace-path occurrences. Current tracked text uses `<workspace>`. This is historical developer-machine metadata, not company data or a credential; no history rewrite was performed. |
| Incomplete dependency text selection | The old generator had 52 license-expression fallback sections and missed bundled `Hack-Regular.txt`, `OFL.txt`, `UFL.txt` and r-efi `AUTHORS` notices. Repaired selection, preserved exact archive texts and added 31 upstream documents; the supplement manifest tracks 21 pinned revisions, including the remaining dispatch gap. The generator verifies all archive checksums against Cargo.lock, supplemental VCS/file hashes and deterministic output. See [notice provenance and limits](../legal/README.md). |
| Remaining dispatch attribution gap | `dispatch 0.2.0` declares MIT and Steven Sheldon author metadata but supplies no crate-specific copyright/license notice in the checked archive/pinned tree. Exact metadata and unchanged official [MIT standard text](https://spdx.org/licenses/MIT.txt) are included; no copyright attribution is invented. Strict `--require-complete` intentionally fails. |
| Upstream objc2 licensing caveat | Ten newer objc2-family packages' pinned `LICENSE.md` documents discuss Apple SDK-derived binding licensing uncertainty. Original declarations/caveats remain verbatim; standard MIT text is included separately. This is unresolved upstream legal uncertainty, not a missing-text issue that this review can certify away. Exact packages/versions are recorded in [coverage](../legal/notice-coverage.json). |
| Product/source claims | README, AGENTS, NOTICE, parity, gaps and security docs consistently describe a clean-room synthetic prototype and exclude affiliation/API compatibility, authenticated identities, payments, ERP writes, accounting compliance and production/full-parity claims. “Never-crash” describes a contributor playbook goal, not a demonstrated universal guarantee. NOTICE identifies the GridCraft reference commit and says no source/assets were copied. This review found no contradictory tracked evidence; it is not an independent proof of authorship. |
| Fresh-checkout instructions | Original README omitted native compiler/system-library and Python prerequisites; these were reported to the maintainer for the stabilization update. Cargo.lock, workspace manifests, `.cargo/config.toml` and the three-platform CI workflow provide pinned build/gate evidence. A new macOS checkout/target smoke check is recorded below; no fresh Windows/Linux machine or interactive GUI was exercised by this audit. |

## Reproduction and checks

The notice repair uses Python 3.11+ standard library only. With locked registry archives cached:

```sh
python scripts/dependency_notices.py --check
python scripts/dependency_notices.py --check --require-complete
```

The first command passes for all 313 locked external packages. The second is an expected failure until the dispatch attribution gap is resolved; it does not modify generated outputs. Focused checks also rejected a wrong crate checksum, a changed supplemental license hash and four portable traversal/absolute-path cases. Original license whitespace is preserved.

Fresh-source validation: a `git archive` of the reviewed commit was expanded into a newly created temporary directory, with a separate empty Cargo target directory and existing registry cache. `cargo metadata --locked --offline` recognized all nine workspace packages. `cargo run --locked --offline -p copperline-cli -- --data <temporary-data> demo` built and seeded revision 1 with six synthetic products; `python3 scripts/workload_paths_test.py` passed. This test uses the existing macOS toolchain, not an empty machine or a claim of bit-reproducible binaries.

The review adds no signing, deployment, telemetry, external business writes or new runtime dependencies. The parent independent M9 review subsequently found no new high-confidence P1/P2 blocker. Final stabilization retains these upstream notice limitations; updated platform/build evidence is tracked in evidence/verification.md.
