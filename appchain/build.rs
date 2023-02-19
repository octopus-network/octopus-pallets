use serde::Deserialize;
use std::{
	borrow::Cow,
	fs,
	io::{BufReader, Read},
	process::Command,
};
use url::Url;

fn main() -> anyhow::Result<()> {
	// Make git hash available via GIT_HASH build-time env var:
	output_git_short_hash();

	output_near_rpc_endpoint()
}

fn output_git_short_hash() {
	let output = Command::new("git").args(["rev-parse", "HEAD"]).output();

	let git_hash = match output {
		Ok(o) if o.status.success() => {
			let sha = String::from_utf8_lossy(&o.stdout).trim().to_owned();
			Cow::from(sha)
		},
		Ok(o) => {
			println!("cargo:warning=Git command failed with status: {}", o.status);
			Cow::from("unknown")
		},
		Err(err) => {
			println!("cargo:warning=Failed to execute git command: {}", err);
			Cow::from("unknown")
		},
	};

	println!("cargo:rustc-env=GIT_HASH={}", git_hash);
	println!("cargo:rerun-if-changed=../.git/HEAD");
	println!("cargo:rerun-if-changed=../.git/refs");
	println!("cargo:rerun-if-changed=build.rs");
}

#[derive(Debug, Deserialize)]
pub struct Config {
	near: Option<NearConfig>,
}

#[derive(Debug, Deserialize)]
struct NearConfig {
	#[serde(deserialize_with = "deserialize_from_str")]
	mainnet_rpc_endpoint: Url,
	#[serde(deserialize_with = "deserialize_from_str")]
	testnet_rpc_endpoint: Url,
}

impl Default for NearConfig {
	fn default() -> Self {
		Self {
			mainnet_rpc_endpoint: Url::parse("https://rpc.mainnet.near.org")
				.expect("Never failed. q;w"),
			testnet_rpc_endpoint: Url::parse("https://rpc.testnet.near.org")
				.expect("Never failed. q;w"),
		}
	}
}

fn deserialize_from_str<'de, S, D>(deserializer: D) -> Result<S, D::Error>
where
	S: std::str::FromStr,
	D: serde::Deserializer<'de>,
	<S as std::str::FromStr>::Err: ToString,
{
	let amount_str: String = Deserialize::deserialize(deserializer)?;
	amount_str.parse::<S>().map_err(|e| serde::de::Error::custom(e.to_string()))
}
fn output_near_rpc_endpoint() -> anyhow::Result<()> {
	let file = fs::File::open("config.toml")?;
	let mut buf_reader = BufReader::new(file);
	let mut contents = String::new();
	buf_reader.read_to_string(&mut contents)?;
	println!("content: {:?}", contents);

	let config: Config = toml::from_str(&contents)?;
	if let Some(config) = config.near {
		println!("cargo:rustc-env=NEAR_MAINNET={}", config.mainnet_rpc_endpoint);
		println!("cargo:rustc-env=NEAR_TESTNET={}", config.testnet_rpc_endpoint);
	} else {
		println!("cargo:rustc-env=NEAR_MAINNET={}", NearConfig::default().mainnet_rpc_endpoint);
		println!("cargo:rustc-env=NEAR_TESTNET={}", NearConfig::default().testnet_rpc_endpoint);
	}
	println!("cargo:rerun-if-changed=../.git/HEAD");
	println!("cargo:rerun-if-changed=../.git/refs");
	println!("cargo:rerun-if-changed=build.rs");

	Ok(())
}
