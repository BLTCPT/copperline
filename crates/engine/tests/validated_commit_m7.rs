//! The validation-token commit path preserves engine transaction and retry semantics.
use copperline_engine::{Command, Engine, Request};
use copperline_model::{Actor, Date, PriceRequest, Role};
use copperline_money::{Bps, Quantity};
use std::{fs, path::PathBuf};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-validated-engine-{}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn request(revision: u64, name: &str, role: Role, action: Command) -> Request {
    Request {
        request_id: format!("validation-token-{revision}"),
        expected_revision: revision,
        actor: Actor {
            name: name.into(),
            role,
        },
        action,
    }
}

#[test]
fn memory_and_disk_agree_after_failed_commit_retry_and_reopen() -> TestResult {
    let workspace = Workspace::new()?;
    let mut durable = Engine::open(&workspace.0)?;
    let mut memory = Engine::memory();
    let date = Date::parse("2026-10-13")?;
    for command in [
        request(0, "admin", Role::Admin, Command::SeedDemo),
        request(
            1,
            "seller",
            Role::Sales,
            Command::QuoteCreate {
                id: "Q-TOKEN".into(),
                title: "Synthetic validation token regression".into(),
                customer_id: "C-ATLAS".into(),
                date: date.clone(),
            },
        ),
    ] {
        durable.execute(command.clone())?;
        memory.execute(command)?;
        assert_eq!(durable.state(), memory.state());
    }
    let line = request(
        2,
        "seller",
        Role::Sales,
        Command::QuoteAddLine {
            id: "Q-TOKEN".into(),
            version: 1,
            request: PriceRequest {
                product_id: "VLV-100".into(),
                customer_id: "C-ATLAS".into(),
                date,
                quantity: Quantity(1_000),
                discount: Bps(0),
                rebate: Bps(0),
                source: None,
                conversion: None,
            },
        },
    );
    let before = durable.state().clone();
    let before_bytes = fs::read(workspace.0.join("00000000000000000002.json"))?;
    let pending = workspace.0.join("00000000000000000003.pending");
    fs::write(&pending, b"preserve interrupted write")?;
    assert!(durable.execute(line.clone()).is_err());
    assert_eq!(durable.state(), &before);
    assert_eq!(fs::read(&pending)?, b"preserve interrupted write");
    assert_eq!(
        fs::read(workspace.0.join("00000000000000000002.json"))?,
        before_bytes
    );
    assert!(!workspace.0.join("00000000000000000003.json").exists());

    fs::remove_file(pending)?;
    assert!(!durable.execute(line.clone())?.replayed);
    memory.execute(line.clone())?;
    assert_eq!(durable.state(), memory.state());
    let committed = fs::read(workspace.0.join("00000000000000000003.json"))?;
    drop(durable);
    let mut reopened = Engine::open(&workspace.0)?;
    assert_eq!(reopened.state(), memory.state());
    let outcome = reopened.execute(line.clone())?;
    assert!(outcome.replayed);
    assert_eq!(outcome.revision, 3);
    let mut conflict = line;
    conflict.actor.name = "other seller".into();
    assert!(reopened.execute(conflict).is_err());
    assert_eq!(reopened.state(), memory.state());
    assert_eq!(
        fs::read(workspace.0.join("00000000000000000003.json"))?,
        committed
    );
    assert!(!workspace.0.join("00000000000000000004.json").exists());
    assert!(!workspace.0.join("00000000000000000004.pending").exists());
    Ok(())
}
