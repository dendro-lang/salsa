//! Basic deletion test:
//!
//! * entities not created in a revision are deleted, as is any memoized data keyed on them.

use salsa::DebugWithDb;
use dendro_salsa_tests::{HasLogger, Logger};

use expect_test::expect;
use test_log::test;

#[salsa::jar(db = Db)]
struct Jar(
    MyInput,
    MyTracked,
    final_result,
    create_tracked_structs,
    contribution_from_struct,
);

trait Db: salsa::DbWithJar<Jar> + HasLogger {}

#[salsa::input]
struct MyInput {
    field: u32,
}

#[salsa::tracked]
fn final_result(db: &dyn Db, input: MyInput) -> u32 {
    db.push_log(format!("final_result({:?})", input));
    let mut sum = 0;
    for tracked_struct in create_tracked_structs(db, input) {
        sum += contribution_from_struct(db, tracked_struct);
    }
    sum
}

#[salsa::tracked]
struct MyTracked {
    field: u32,
}

#[salsa::tracked]
fn create_tracked_structs(db: &dyn Db, input: MyInput) -> Vec<MyTracked> {
    db.push_log(format!("intermediate_result({:?})", input));
    (0..input.field(db))
        .map(|i| MyTracked::new(db, i))
        .collect()
}

#[salsa::tracked]
fn contribution_from_struct(db: &dyn Db, tracked: MyTracked) -> u32 {
    tracked.field(db) * 2
}

#[salsa::db(Jar)]
#[derive(Default)]
struct Database {
    storage: salsa::Storage<Self>,
    logger: Logger,
}

impl salsa::Database for Database {
    fn salsa_event(&self, event: salsa::Event) {
        match event.kind {
            salsa::EventKind::WillDiscardStaleOutput { .. }
            | salsa::EventKind::DidDiscard { .. } => {
                self.push_log(format!("salsa_event({:?})", event.kind.debug(self)));
            }
            _ => {}
        }
    }
}

impl Db for Database {}

impl HasLogger for Database {
    fn logger(&self) -> &Logger {
        &self.logger
    }
}

#[test]
fn basic() {
    let mut db = Database::default();

    // Creates 3 tracked structs
    let input = MyInput::new(&db, 3);
    assert_eq!(final_result(&db, input), 2 * 2 + 2);
    db.assert_logs(expect![[r#"
        [
            "final_result(MyInput(Id { value: 1 }))",
            "intermediate_result(MyInput(Id { value: 1 }))",
        ]"#]]);

    // Creates only 2 tracked structs in this revision, should delete 1
    //
    // Expect to see 3 DidDiscard events--
    //
    // * the struct itself
    // * the struct's field
    // * the `contribution_from_struct` result
    input.set_field(&mut db).to(2);
    assert_eq!(final_result(&db, input), 2);
    db.assert_logs(expect![[r#"
        [
            "intermediate_result(MyInput(Id { value: 1 }))",
            "salsa_event(WillDiscardStaleOutput { execute_key: create_tracked_structs(0), output_key: MyTracked(2) })",
            "salsa_event(DidDiscard { key: MyTracked(2) })",
            "salsa_event(DidDiscard { key: field(2) })",
            "salsa_event(DidDiscard { key: contribution_from_struct(2) })",
            "salsa_event(WillDiscardStaleOutput { execute_key: create_tracked_structs(0), output_key: field(2) })",
            "final_result(MyInput(Id { value: 1 }))",
        ]"#]]);
}
