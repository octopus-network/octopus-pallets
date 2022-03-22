use std::{fs::File, io::prelude::*};

fn get_git_hash() -> Option<String> {
	use std::process::Command;
	let commit = Command::new("git").arg("rev-parse").arg("HEAD").output();
	if let Ok(commit_output) = commit {
		let commit_hash = String::from_utf8_lossy(&commit_output.stdout);
		return Some(commit_hash.into())
	}
	None
}

fn set_version_to_octopus_pallet() {
	let version = get_git_hash().expect("Get commit hash error.");
	let mut f = File::create("./src/version.rs").unwrap();

	let mut content = br#"
use super::*;
impl<T: Config> Pallet<T> {
    pub(super) fn set_version() {"#
		.to_vec();

	content.extend(
		br#"
        let v = 
        ""#,
	);
	content.extend(&version.as_bytes().to_vec());
	content.extend(br#"".trim();"#);
	content.extend(
		br#"
		let v1 = hex::decode(v).expect("Decoding failed");"#,
	);
	content.extend(
		br#"
		<OctopusPalletVersion<T>>::put(v1);"#,
	);
	content.extend(
		br#"
    }
}"#,
	);

	f.write_all(&content).unwrap();
}

fn main() {
	set_version_to_octopus_pallet();
}
