use xx_core::test::async_tests;
use xx_pulse::*;

#[main]
#[test]
async fn test_add() {
	async_tests::test_add().await;
}
