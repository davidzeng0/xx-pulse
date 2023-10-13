use std::{
	io::{Result, SeekFrom},
	os::fd::{AsFd, OwnedFd},
	path::Path
};

use xx_core::{
	async_std::io::{Close, Read, Seek, Write},
	coroutines::async_trait_fn,
	os::fcntl::OpenFlag
};

use crate::{async_runtime::*, ops::io::*};

pub struct File {
	fd: OwnedFd,
	offset: u64
}

#[async_fn]
impl File {
	pub async fn open(path: impl AsRef<Path>) -> Result<File> {
		Ok(File {
			fd: open(path.as_ref(), OpenFlag::ReadOnly, 0).await?,
			offset: 0
		})
	}

	pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
		let read = read(self.fd.as_fd(), buf, self.offset as i64).await?;

		self.offset += read as u64;

		Ok(read)
	}

	pub async fn write(&mut self, buf: &[u8]) -> Result<usize> {
		let wrote = write(self.fd.as_fd(), buf, self.offset as i64).await?;

		self.offset += wrote as u64;

		Ok(wrote)
	}

	pub async fn flush(&mut self) -> Result<()> {
		todo!();
	}

	pub async fn seek(&mut self, seek: SeekFrom) -> Result<u64> {
		match seek {
			SeekFrom::Start(pos) => self.offset = pos,
			SeekFrom::Current(rel) => self.offset = self.offset.wrapping_add_signed(rel),
			SeekFrom::End(_) => todo!()
		}

		Ok(self.offset)
	}

	pub async fn stream_position(&mut self) -> Result<u64> {
		Ok(self.offset)
	}

	pub async fn close(self) -> Result<()> {
		close(self.fd).await
	}
}

#[async_trait_fn]
impl Read<Context> for File {
	async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
		self.read(buf).await
	}
}

#[async_trait_fn]
impl Write<Context> for File {
	async fn write(&mut self, buf: &[u8]) -> Result<usize> {
		self.write(buf).await
	}

	async fn flush(&mut self) -> Result<()> {
		self.flush().await
	}
}

#[async_trait_fn]
impl Seek<Context> for File {
	async fn seek(&mut self, seek: SeekFrom) -> Result<u64> {
		self.seek(seek).await
	}

	fn stream_position_fast(&self) -> bool {
		true
	}

	async fn stream_position(&mut self) -> Result<u64> {
		self.stream_position().await
	}
}

#[async_trait_fn]
impl Close<Context> for File {
	async fn close(self) -> Result<()> {
		self.close().await
	}
}
