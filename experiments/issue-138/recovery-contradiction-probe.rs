use link_cli::{
    CommitMode, GenericTransactionsDecorator, LinkError, LinkStorage, LogRetentionPolicy,
    TransitionLogStore,
};

#[derive(Default)]
struct Journal(Vec<String>);
impl TransitionLogStore for Journal {
    fn append_log_entry(&mut self, record: &str) -> Result<(), LinkError> {
        self.0.push(record.to_owned());
        Ok(())
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.0.clone())
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        Ok(())
    }
}
fn transition(id: u128, sequence: i64, after: u32) -> String {
    format!("__transactions:transition:v1|{id:032x}|{sequence}|0|1|1,11,12|1,{after},22")
}
fn commit(id: u128) -> String {
    format!("__transactions:commit:{id:032x}")
}
fn rollback(id: u128) -> String {
    format!("__transactions:rollback:{id:032x}")
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        (
            "commit then abort",
            vec![transition(1, 1, 21), commit(1), rollback(1)],
        ),
        (
            "abort then commit",
            vec![transition(1, 1, 21), rollback(1), commit(1)],
        ),
        (
            "different owners of one sequence",
            vec![
                transition(1, 1, 21),
                commit(1),
                transition(2, 1, 31),
                commit(2),
            ],
        ),
        (
            "different payloads for one sequence",
            vec![transition(1, 1, 21), transition(1, 1, 31), commit(1)],
        ),
        ("zero sequence", vec![transition(1, 0, 21), commit(1)]),
        ("negative sequence", vec![transition(1, -1, 21), commit(1)]),
        ("zero owner", vec![transition(0, 1, 21), commit(0)]),
    ];
    let mut failures = 0;
    for (label, entries) in cases {
        let mut rows = LinkStorage::in_memory(false);
        let row = rows.create(11, 12);
        let mut tx = GenericTransactionsDecorator::<u32, _, _>::new(
            rows,
            Journal::default(),
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )?;
        for entry in &entries {
            tx.log_store_mut().append_log_entry(entry)?;
        }
        let result = tx.recover();
        let actual = tx.get(row).unwrap().source;
        let unchanged = tx.log_store_mut().read_log_entries()? == entries;
        let correct = result.is_err() && actual == 11 && unchanged;
        println!("{label}: rejected={}, row={actual} (expected 11), journal unchanged={unchanged}, contract={correct}",result.is_err());
        if !correct {
            failures += 1;
        }
    }
    println!("Finite contradiction contracts failing: {failures}/7");
    std::process::exit(i32::from(failures != 0));
}
