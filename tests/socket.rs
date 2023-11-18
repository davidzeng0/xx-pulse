use xx_core::error::*;
use xx_pulse::*;

#[main]
#[test]
async fn test_udp() -> Result<()> {
	let server = Udp::bind("0.0.0.0:0").await?;
	let client = Udp::connect(server.local_addr().await?).await?;

	let mut buf = [0u8; 1];

	for i in 0..10 {
		buf[0] = i;
		client.send(&buf, 0).await?;
		buf[0] = 255;
		server.recv(&mut buf, 0).await?;

		assert!(buf[0] == i);
	}

	Ok(())
}

#[main]
#[test]
async fn test_tcp() -> Result<()> {
	let listener = Tcp::bind("0.0.0.0:0").await?;
	let Join((server, addr), client) = join(
		listener.accept(),
		Tcp::connect(listener.local_addr().await?)
	)
	.await
	.flatten()?;

	let client_addr = client.local_addr().await?;

	assert_eq!(addr, client_addr);

	let mut buf = [0u8; 1];

	for i in 0..10 {
		buf[0] = i;
		client.send(&buf, 0).await?;
		buf[0] = 255;
		server.recv(&mut buf, 0).await?;

		assert!(buf[0] == i);
	}

	Ok(())
}
