//! Thin shell: pick a directory, print it on stdout for the shell wrapper.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or(std::env::current_dir()?)
        .canonicalize()?;
    if let Some(dir) = cdt_view::pick(root)? {
        println!("{}", dir.display());
    }
    Ok(())
}
