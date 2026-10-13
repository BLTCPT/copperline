//! Independent regression coverage for generated requests and durable retry semantics.
use copperline_engine::{Command, Engine, Request};
use copperline_model::*;
use std::{fs, path::PathBuf};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
struct Workspace(PathBuf);
impl Workspace {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "copperline-persistence-m3-review-{}",
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
fn actor() -> Actor {
    Actor {
        name: "Synthetic reviewer".into(),
        role: Role::Admin,
    }
}

#[test]
fn local_commands_skip_reserved_keys_and_external_retries_survive_reopen() -> TestResult {
    let workspace = Workspace::new()?;
    let mut engine = Engine::open(&workspace.0)?;
    engine.run(actor(), Command::SeedDemo)?;
    // Both the base ID and first collision suffix were previously used by a different client.
    let mut external = Vec::new();
    for key in ["local-4-quote_create", "local-4-quote_create-1"] {
        let request = Request {
            request_id: key.into(),
            expected_revision: engine.state().revision,
            actor: actor(),
            action: Command::CustomerPut(Customer {
                id: "C-ATLAS".into(),
                name: "Synthetic client import".into(),
                segment: "OEM".into(),
            }),
        };
        engine.execute(request.clone())?;
        external.push(request);
    }
    let result = engine.run(
        actor(),
        Command::QuoteCreate {
            id: "Q-RESERVED".into(),
            title: "Synthetic collision regression".into(),
            customer_id: "C-ATLAS".into(),
            date: Date::parse("2026-10-13")?,
        },
    )?;
    assert!(!result.replayed);
    assert_eq!(result.revision, 4);
    assert_eq!(engine.state().quotes.len(), 1);
    let expected = engine.state().clone();
    drop(engine);
    let mut reopened = Engine::open(&workspace.0)?;
    assert_eq!(reopened.state(), &expected);
    for request in external {
        let receipt = reopened.execute(request.clone())?;
        assert!(receipt.replayed);
        assert_eq!(receipt.revision, request.expected_revision + 1);
        assert_eq!(reopened.state(), &expected);
        let mut changed = request;
        changed.actor.name = "Different person".into();
        assert!(reopened.execute(changed).is_err());
        assert_eq!(reopened.state(), &expected);
    }
    Ok(())
}
