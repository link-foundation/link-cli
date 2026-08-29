// Isolates the external-reference range `platform-data` 2.0.0 builds, which
// `doublets` 0.5.0 re-exports as `doublets::data::LinksConstants`.
//
// `LinksConstants::full_new` reserves six service values at the top of the
// internal range and then takes the external range verbatim:
//
//     r#continue: *internal.end(),
//     r#break:    *internal.end() - 1,
//     ...
//     internal_range: *internal.start()..=*internal.end() - 6,
//     external_range: external,
//
// With external references enabled the defaults are
// `internal = 1..=half` and `external = half..=MAX`, so `external_range`
// *starts on* `r#continue` -- the two overlap by one address.
//
// The C# `LinksConstants<TLinkAddress>` this mirrors starts the external range
// one past the half instead (see ../csharp), so no service constant is ever
// reported as an external reference.
//
// Exits 0 while the overlap reproduces, and non-zero once upstream fixes it.
use doublets::data::LinksConstants;

fn main() {
    let c = LinksConstants::<u32>::external();
    println!("null      = {}", c.null);
    println!("continue  = {}", c.r#continue);
    println!("break     = {}", c.r#break);
    println!("skip      = {}", c.skip);
    println!("any       = {}", c.any);
    println!("itself    = {}", c.itself);
    println!("error     = {}", c.error);
    println!("internal  = {:?}", c.internal_range);
    println!("external  = {:?}", c.external_range);

    let overlaps = c.is_external(c.r#continue);
    println!();
    println!("is_external(continue) = {overlaps}");
    println!(
        "expected              = false (C# reports False for the same query)"
    );

    if overlaps {
        println!("\nReproduced: the external range still starts on `continue`.");
    } else {
        println!("\nFixed upstream: the ranges no longer overlap.");
        std::process::exit(1);
    }
}
