use copperline_engine::{Command, Engine, Request, compare, evaluate, export_quote};
use copperline_model::*;
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
const MAX_CONNECTIONS: usize = 16;
const MAX_REQUEST_BYTES: u64 = 2 * 1024 * 1024;
fn main() {
    if let Err(e) = run() {
        eprintln!("Copperline: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "--version") {
        println!("Copperline CLI {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let mut i = 0;
    let mut path = PathBuf::from("data/demo");
    if args.first().is_some_and(|s| s == "--data") {
        path = PathBuf::from(
            args.get(1)
                .ok_or_else(|| error("--data requires a directory"))?,
        );
        i = 2;
    }
    let verb = args.get(i).map(String::as_str).unwrap_or("help");
    if verb == "help" {
        println!(
            "Copperline pricing workbench\nUsage: copperline-cli [--data DIR] demo|state|execute JSON|api|serve PORT|export-quote ID FILE|export-settlement ID FILE|export-contract ID EDITION FILE|export-conversion ID FILE|export-report ID FILE|export-booking ID FILE|preview-booking JSON|commands\nRecovery: storage-inspect | quarantine-pending NAME SHA256\napi: bounded JSON lines over stdin/stdout. serve: opt-in loopback JSON lines, maximum 16 clients, no remote authentication.\nRecovery verifies committed history and preserves pending evidence; it never promotes an interrupted write.\nAll figures and identities in the demo are synthetic.\n--version prints the build version."
        );
        println!(
            "Backup: --data EXISTING_DIR backup NEW_ARCHIVE.clbak | backup-inspect ARCHIVE.clbak | restore ARCHIVE.clbak NEW_WORKSPACE_DIR\nRestore never replaces an existing directory. Backup and restore include pending/recovery evidence without promoting it."
        );
        return Ok(());
    }
    if matches!(verb, "backup" | "backup-inspect" | "restore") {
        let archive = args
            .get(i + 1)
            .ok_or_else(|| error("Backup commands require an archive path"))?;
        let result = match verb {
            "backup" => serde_json::to_value(Engine::backup_workspace(&path, archive)?),
            "backup-inspect" => serde_json::to_value(Engine::inspect_backup(archive)?),
            _ => {
                let destination = args
                    .get(i + 2)
                    .ok_or_else(|| error("restore requires ARCHIVE NEW_WORKSPACE_DIR"))?;
                serde_json::to_value(Engine::restore_backup(archive, destination)?)
            }
        }
        .map_err(|e| error(e.to_string()))?;
        println!(
            "{}",
            serde_json::to_string_pretty(&result).map_err(|e| error(e.to_string()))?
        );
        return Ok(());
    }
    if matches!(verb, "storage-inspect" | "quarantine-pending") {
        let (store, _state) = copperline_storage::Store::open(&path)?;
        let result = if verb == "storage-inspect" {
            serde_json::to_value(store.inspect_pending()?)
        } else {
            let name = args.get(i + 1).ok_or_else(|| {
                error("quarantine-pending requires the exactly-next pending NAME")
            })?;
            let sha256 = args.get(i + 2).ok_or_else(|| {
                error("quarantine-pending requires the SHA256 returned by storage-inspect")
            })?;
            serde_json::to_value(store.quarantine_pending(name, sha256)?)
        }
        .map_err(|e| error(e.to_string()))?;
        println!(
            "{}",
            serde_json::to_string_pretty(&result).map_err(|e| error(e.to_string()))?
        );
        return Ok(());
    }
    let mut e = Engine::open(path)?;
    match verb {
        "demo" => {
            if e.state().products.is_empty() {
                e.run(
                    Actor {
                        name: "Demo administrator".into(),
                        role: Role::Admin,
                    },
                    Command::SeedDemo,
                )?;
            }
            println!(
                "{}",
                json!({"revision":e.state().revision,"products":e.state().products.len(),"workspace":e.path()})
            );
        }
        "state" => println!(
            "{}",
            serde_json::to_string_pretty(e.state()).map_err(|e| error(e.to_string()))?
        ),
        "execute" => {
            let input = args
                .get(i + 1)
                .ok_or_else(|| error("execute requires a JSON request"))?;
            let request: Request = serde_json::from_str(input).map_err(|e| error(e.to_string()))?;
            println!(
                "{}",
                serde_json::to_string(&e.execute(request)?).map_err(|e| error(e.to_string()))?
            );
        }
        "preview-booking" => {
            let input = args
                .get(i + 1)
                .ok_or_else(|| error("preview-booking requires BookingRequest JSON"))?;
            let request = serde_json::from_str(input).map_err(|e| error(e.to_string()))?;
            println!(
                "{}",
                serde_json::to_string(&e.state().preview_quote_booking(&request)?)
                    .map_err(|e| error(e.to_string()))?
            );
        }
        "export-contract" => {
            let id = args
                .get(i + 1)
                .ok_or_else(|| error("export-contract requires ID EDITION FILE"))?;
            let edition = args
                .get(i + 2)
                .ok_or_else(|| error("Missing contract edition"))?
                .parse::<u64>()
                .map_err(|_| error("Invalid contract edition"))?;
            let path = args
                .get(i + 3)
                .ok_or_else(|| error("Missing export path"))?;
            let csv = copperline_engine::export_contract(e.state(), id, edition)?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|e| error(e.to_string()))?;
            file.write_all(csv.as_bytes())
                .map_err(|e| error(e.to_string()))?;
            println!("Exported approved contract evidence");
        }
        "commands" => println!(
            "seed_demo quote_book conversion_create conversion_import conversion_submit conversion_decide conversion_withdraw report_convert quote_replace_lines contract_create contract_submit contract_decide contract_withdraw correction_create correction_submit correction_decide correction_withdraw settlement_create settlement_submit settlement_decide settlement_withdraw grid_calculate grid_submit grid_decide agreement_create agreement_submit agreement_decide transaction_add transactions_import rebate_accrue product_put customer_put cost_add rule_add policy_set quote_approval_policy_set products_import quote_create quote_add_line quote_replace_line quote_remove_line quote_reprice quote_submit quote_decide quote_revise list_calculate list_submit list_decide list_publish\nRead API: state evaluate compare preview_quote_booking export_booking converted_actuals export_conversion export_report export_quote export_contract export_settlement reconciliation analytics catalog"
        ),
        "api" => {
            let stdin = io::stdin();
            let stdout = io::stdout();
            lines(&mut e, &mut stdin.lock(), &mut stdout.lock())?;
        }
        "serve" => {
            let port = args
                .get(i + 1)
                .ok_or_else(|| error("serve requires a port"))?
                .parse::<u16>()
                .map_err(|_| error("Invalid port"))?;
            ensure(port >= 1024, "Use a port from 1024 to 65535")?;
            let listener =
                TcpListener::bind(("127.0.0.1", port)).map_err(|e| error(e.to_string()))?;
            eprintln!(
                "Copperline local JSON API listening on 127.0.0.1:{port}; local trusted processes only"
            );
            let engine = Arc::new(Mutex::new(e));
            let active = Arc::new(AtomicUsize::new(0));
            for client in listener.incoming() {
                match client {
                    Ok(mut stream) => {
                        if active
                            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                                (n < MAX_CONNECTIONS).then_some(n + 1)
                            })
                            .is_err()
                        {
                            // Bound both threads and memory. A busy response never takes the engine lock.
                            let _ =
                                stream.set_write_timeout(Some(std::time::Duration::from_secs(5)));
                            let _ = write_response(
                                &mut stream,
                                &json!({"ok":false,"error":"Local API connection limit reached; retry after a client closes"}),
                            );
                            let _ = stream.shutdown(Shutdown::Write);
                            // Consume one ordinary request already in flight before close to avoid
                            // resetting the socket and discarding the busy response. One bounded
                            // read only: rejected clients cannot occupy a worker or retain the loop.
                            let _ = stream
                                .set_read_timeout(Some(std::time::Duration::from_millis(100)));
                            let _ = stream.read(&mut [0; 4096]);
                            continue;
                        }
                        let permit = ConnectionPermit(Arc::clone(&active));
                        let engine = Arc::clone(&engine);
                        if let Err(err) = std::thread::Builder::new()
                            .name("copperline-local-client".into())
                            .spawn(move || {
                                let _permit = permit;
                                if let Err(err) = serve(engine, stream) {
                                    eprintln!("Connection: {err}");
                                }
                            })
                        {
                            eprintln!("Unable to start connection: {err}");
                        }
                    }
                    Err(err) => eprintln!("Accept: {err}"),
                }
            }
        }
        "export-quote" | "export-settlement" | "export-conversion" | "export-report"
        | "export-booking" => {
            let id = args.get(i + 1).ok_or_else(|| error("Missing quote ID"))?;
            let path = args
                .get(i + 2)
                .ok_or_else(|| error("Missing export path"))?;
            let content = if verb == "export-booking" {
                copperline_engine::export_booking(e.state(), id)?
            } else if verb == "export-conversion" {
                copperline_engine::export_conversion(e.state(), id)?
            } else if verb == "export-report" {
                copperline_engine::export_report(e.state(), id)?
            } else if verb == "export-settlement" {
                copperline_engine::export_settlement(e.state(), id)?
            } else {
                let q = e
                    .state()
                    .quotes
                    .get(id)
                    .ok_or_else(|| error("Quote not found"))?;
                export_quote(q)?
            };
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|e| error(e.to_string()))?;
            file.write_all(content.as_bytes())
                .map_err(|e| error(e.to_string()))?;
            println!("Exported {id} to {path}");
        }
        _ => return Err(error("Unknown command; run help")),
    }
    Ok(())
}
struct ConnectionPermit(Arc<AtomicUsize>);
impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
fn serve(engine: Arc<Mutex<Engine>>, stream: TcpStream) -> Result<()> {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|e| error(e.to_string()))?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|e| error(e.to_string()))?;
    let mut writer = stream.try_clone().map_err(|e| error(e.to_string()))?;
    let mut reader = BufReader::new(stream);
    while let Some(bytes) = read_line(&mut reader)? {
        let response = {
            let mut e = engine.lock().map_err(|_| {
                error(
                    "Engine unavailable after an unexpected failure; restart and inspect workspace",
                )
            })?;
            response(&mut e, &bytes)
        };
        // Socket reads/writes never hold the engine mutex. Stalled clients cannot block another command.
        write_response(&mut writer, &response)?;
    }
    Ok(())
}
fn lines(e: &mut Engine, reader: &mut impl BufRead, writer: &mut impl Write) -> Result<()> {
    while let Some(bytes) = read_line(reader)? {
        write_response(writer, &response(e, &bytes))?;
    }
    Ok(())
}
fn read_line(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    let count = reader
        .take(MAX_REQUEST_BYTES + 1)
        .read_until(b'\n', &mut bytes)
        .map_err(|e| error(e.to_string()))?;
    if count == 0 {
        return Ok(None);
    }
    ensure(
        count as u64 <= MAX_REQUEST_BYTES,
        "Request line exceeds 2 MiB; connection closed",
    )?;
    Ok(Some(bytes))
}
fn response(e: &mut Engine, bytes: &[u8]) -> Value {
    let result = serde_json::from_slice::<Value>(bytes)
        .map_err(|e| error(e.to_string()))
        .and_then(|v| handle(e, v));
    match result {
        Ok(value) => json!({"ok":true,"result":value}),
        Err(err) => json!({"ok":false,"error":err.to_string(),"revision":e.state().revision}),
    }
}
fn write_response(writer: &mut impl Write, response: &Value) -> Result<()> {
    serde_json::to_writer(&mut *writer, response).map_err(|e| error(e.to_string()))?;
    writer.write_all(b"\n").map_err(|e| error(e.to_string()))?;
    writer.flush().map_err(|e| error(e.to_string()))?;
    Ok(())
}
fn handle(e: &mut Engine, v: Value) -> Result<Value> {
    let method = v
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| error("Missing method"))?;
    let params = v.get("params").cloned().unwrap_or(Value::Null);
    match method {
        "state" => serde_json::to_value(e.state()).map_err(|e| error(e.to_string())),
        "execute" => {
            let r: Request = serde_json::from_value(params).map_err(|e| error(e.to_string()))?;
            serde_json::to_value(e.execute(r)?).map_err(|e| error(e.to_string()))
        }
        "evaluate" => {
            let r = serde_json::from_value(params).map_err(|e| error(e.to_string()))?;
            serde_json::to_value(evaluate(e.state(), &r)?).map_err(|e| error(e.to_string()))
        }
        "compare" => {
            let baseline =
                serde_json::from_value(params.get("baseline").cloned().unwrap_or(Value::Null))
                    .map_err(|e| error(e.to_string()))?;
            let scenario =
                serde_json::from_value(params.get("scenario").cloned().unwrap_or(Value::Null))
                    .map_err(|e| error(e.to_string()))?;
            serde_json::to_value(compare(e.state(), &baseline, &scenario)?)
                .map_err(|e| error(e.to_string()))
        }
        "export_quote" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| error("Missing quote ID"))?;
            let q = e
                .state()
                .quotes
                .get(id)
                .ok_or_else(|| error("Quote not found"))?;
            Ok(json!({"csv":export_quote(q)?}))
        }
        "converted_actuals" => {
            let request: ReportingRequest =
                serde_json::from_value(params.clone()).map_err(|e| error(e.to_string()))?;
            serde_json::to_value(e.state().converted_actuals(&request, e.state().revision)?)
                .map_err(|e| error(e.to_string()))
        }
        "preview_quote_booking" => {
            let request = serde_json::from_value(params).map_err(|e| error(e.to_string()))?;
            serde_json::to_value(e.state().preview_quote_booking(&request)?)
                .map_err(|e| error(e.to_string()))
        }
        "export_conversion" | "export_report" | "export_booking" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| error("Missing document ID"))?;
            let csv = if method == "export_booking" {
                copperline_engine::export_booking(e.state(), id)?
            } else if method == "export_conversion" {
                copperline_engine::export_conversion(e.state(), id)?
            } else {
                copperline_engine::export_report(e.state(), id)?
            };
            Ok(json!({"csv":csv}))
        }
        "export_contract" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| error("Missing contract ID"))?;
            let edition = params
                .get("edition")
                .and_then(Value::as_u64)
                .ok_or_else(|| error("Missing contract edition"))?;
            Ok(json!({"csv":copperline_engine::export_contract(e.state(),id,edition)?}))
        }
        "export_settlement" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| error("Missing settlement ID"))?;
            Ok(json!({"csv":copperline_engine::export_settlement(e.state(),id)?}))
        }
        "reconciliation" => {
            let id = params
                .get("agreement_id")
                .and_then(Value::as_str)
                .ok_or_else(|| error("Missing agreement ID"))?;
            serde_json::to_value(e.state().rebate_position(id, e.state().revision)?)
                .map_err(|e| error(e.to_string()))
        }
        "analytics" => serde_json::to_value(copperline_engine::analytics(e.state())?)
            .map_err(|e| error(e.to_string())),
        "catalog" => Ok(
            json!({"api_version":1,"money":"integer cents","quantity":"integer thousandths","percent":"integer basis points","methods":["state","execute","evaluate","compare","preview_quote_booking","export_booking","converted_actuals","export_conversion","export_report","export_quote","export_contract","export_settlement","reconciliation","analytics","catalog"]}),
        ),
        _ => Err(error("Unknown API method")),
    }
}
