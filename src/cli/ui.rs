/// Print a styled header
pub fn print_header(title: &str) {
    println!("\n=== {title} ===\n");
}

/// Print a key-value table
pub fn print_table(rows: &[(&str, &str)]) {
    for (key, val) in rows {
        println!("  {key:<20} {val}");
    }
}
