use scale_info::prelude::{format, string::String};

pub fn hex_format(data: &[u8]) -> String {
	format!("0x{}", hex::encode(data))
}
