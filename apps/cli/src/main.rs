use copperline_engine::{Command, Engine, Request, compare, evaluate, export_quote};
use copperline_model::*;
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
};
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
            "Copperline pricing workbench\nUsage: copperline-cli [--data DIR] demo|state|execute JSON|api|serve PORT|export-quote ID FILE|commands\napi: bounded JSON lines over stdin/stdout. serve: opt-in loopback JSON lines, no remote authentication.\nAll figures and identities in the demo are synthetic.\n--version prints the build version."
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
        "commands" => println!(
            "seed_demo grid_calculate grid_submit grid_decide agreement_create agreement_submit agreement_decide transaction_add transactions_import rebate_accrue product_put customer_put cost_add rule_add policy_set products_import quote_create quote_add_line quote_remove_line quote_reprice quote_submit quote_decide quote_revise list_calculate list_submit list_decide list_publish\nRead API: state evaluate compare export_quote analytics catalog"
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
            for client in listener.incoming() {
                match client {
                    Ok(stream) => {
                        if let Err(err) = serve(&mut e, stream) {
                            eprintln!("Connection: {err}");
                        }
                    }
                    Err(err) => eprintln!("Accept: {err}"),
                }
            }
        }
        "export-quote" => {
            let id = args.get(i + 1).ok_or_else(|| error("Missing quote ID"))?;
            let path = args
                .get(i + 2)
                .ok_or_else(|| error("Missing export path"))?;
            let q = e
                .state()
                .quotes
                .get(id)
                .ok_or_else(|| error("Quote not found"))?;
            let content = export_quote(q)?;
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
fn serve(e: &mut Engine, stream: TcpStream) -> Result<()> {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|e| error(e.to_string()))?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|e| error(e.to_string()))?;
    let mut writer = stream.try_clone().map_err(|e| error(e.to_string()))?;
    lines(e, &mut BufReader::new(stream), &mut writer)
}
fn lines(e: &mut Engine, reader: &mut impl BufRead, writer: &mut impl Write) -> Result<()> {
    loop {
        let mut bytes = Vec::new();
        let count = reader
            .take(2 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut bytes)
            .map_err(|e| error(e.to_string()))?;
        if count == 0 {
            return Ok(());
        }
        ensure(
            count <= 2 * 1024 * 1024,
            "Request line exceeds 2 MiB; connection closed",
        )?;
        let result = serde_json::from_slice::<Value>(&bytes)
            .map_err(|e| error(e.to_string()))
            .and_then(|v| handle(e, v));
        let response = match result {
            Ok(value) => json!({"ok":true,"result":value}),
            Err(err) => json!({"ok":false,"error":err.to_string(),"revision":e.state().revision}),
        };
        serde_json::to_writer(&mut *writer, &response).map_err(|e| error(e.to_string()))?;
        writer.write_all(b"\n").map_err(|e| error(e.to_string()))?;
        writer.flush().map_err(|e| error(e.to_string()))?;
    }
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
        "analytics" => serde_json::to_value(copperline_engine::analytics(e.state())?)
            .map_err(|e| error(e.to_string())),
        "catalog" => Ok(
            json!({"api_version":1,"money":"integer cents","quantity":"integer thousandths","percent":"integer basis points","methods":["state","execute","evaluate","compare","export_quote","analytics","catalog"]}),
        ),
        _ => Err(error("Unknown API method")),
    }
}
