#![allow(warnings)]

use xx_core::async_std::io::ReadExt;
use xx_pulse::*;

#[main]
#[test]
async fn test_file() {
	let mut file = File::open("Cargo.toml").await.unwrap();
	let mut str = String::new();

	file.read_to_string(&mut str).await.unwrap();

	assert!(str.contains("[package]"));
}
