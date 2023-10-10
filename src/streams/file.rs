use std::{
	io::Result,
	os::fd::{AsFd, OwnedFd},
	path::Path
};

use xx_core::os::fcntl::OpenFlag;

use crate::{
	async_runtime::*,
	ops::{close, open, read, write}
};

pub struct File {
	fd: OwnedFd
}

#[async_fn]
impl File {
	pub async fn open(path: impl AsRef<Path>) -> Result<File> {
		Ok(File {
			fd: open(path.as_ref(), OpenFlag::ReadOnly, 0).await?
		})
	}

	pub async fn read(&self, buf: &mut [u8], offset: i64) -> Result<usize> {
		read(self.fd.as_fd(), buf, offset).await
	}

	pub async fn write(&self, buf: &[u8], offset: i64) -> Result<usize> {
		write(self.fd.as_fd(), buf, offset).await
	}

	pub async fn close(self) -> Result<()> {
		close(self.fd).await
	}
}
