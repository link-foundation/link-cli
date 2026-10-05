//! Prints how links-notation structures LiNo text, in the same notation as
//! `examples/lino-structure-probe`, so the Rust and C# parsers can
//! be compared line by line: `cargo run --example lino_structure_probe -- 'a b'`.

use links_notation::{parse_lino_to_links, LiNo};

fn show(link: &LiNo<String>) -> String {
    match link {
        LiNo::Ref(reference) => format!("Ref({reference})"),
        LiNo::Link { id, values } => format!(
            "Link{{{},[{}]}}",
            id.as_deref().unwrap_or("None"),
            values.iter().map(show).collect::<Vec<_>>().join(",")
        ),
    }
}

fn main() {
    for input in std::env::args().skip(1) {
        let shown = input.replace('\n', "\\n").replace('\r', "\\r");
        match parse_lino_to_links(&input) {
            Ok(links) => println!(
                "{shown} => [{}]",
                links.iter().map(show).collect::<Vec<_>>().join(", ")
            ),
            Err(error) => println!("{shown} => ERR {error}"),
        }
    }
}
