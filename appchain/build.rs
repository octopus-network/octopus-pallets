use std::{env, fs::File, io::Write, path::Path, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
	write_git_version()
}

fn write_git_version() -> Result<(), Box<dyn std::error::Error>> {
	let maybe_hash = get_git_hash();
	let git_hash = maybe_hash.as_deref().unwrap_or("baaaaaad");

	let dest_path = Path::new(&env::var("OUT_DIR")?).join("git_version");

	let mut file = File::create(&dest_path)?;
	write!(file, "{}", git_hash)?;

	// TODO: are these right?
	println!("cargo:rerun-if-changed=.git/HEAD");
	println!("cargo:rerun-if-changed=.git/index");

	Ok(())
}

fn get_git_hash() -> Option<String> {
	let head = Command::new("git").arg("rev-parse").arg("HEAD").output();
	if let Ok(h) = head {
		let h = String::from_utf8_lossy(&h.stdout).trim().to_string();
		return Some(h)
	}
	None
}
