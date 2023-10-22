use std::{
	io::SeekFrom,
	os::fd::{AsFd, OwnedFd},
	path::Path
};

use xx_core::{
	async_std::io::*,
	coroutines::async_trait_fn,
	error::*,
	os::{
		fcntl::OpenFlag,
		stat::{Statx, StatxMask}
	}
};

use crate::{async_runtime::*, ops::*};

pub struct File {
	fd: OwnedFd,
	offset: u64,
	length: u64
}

#[async_fn]
impl File {
	pub async fn open(path: impl AsRef<Path>) -> Result<File> {
		let mut stat = Statx::new();

		statx(path.as_ref(), 0, 0, &mut stat).await?;

		if stat.mask & (StatxMask::Size as u32) == 0 {
			Err(Error::new(ErrorKind::Other, "Failed to query file size"))
		} else {
			Ok(File {
				fd: open(path.as_ref(), OpenFlag::ReadOnly, 0).await?,
				offset: 0,
				length: stat.size
			})
		}
	}

	pub async fn read(&mut self, mut buf: &mut [u8]) -> Result<usize> {
		let remaining = (self.length - self.offset) as usize;

		if remaining == 0 {
			return Ok(0);
		}

		let min = buf.len().min(remaining);

		buf = unsafe { buf.get_unchecked_mut(0..min) };

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
		fsync(self.fd.as_fd()).await
	}

	pub async fn seek(&mut self, seek: SeekFrom) -> Result<u64> {
		match seek {
			SeekFrom::Start(pos) => self.offset = pos,
			SeekFrom::Current(rel) => self.offset = self.offset.wrapping_add_signed(rel),
			SeekFrom::End(rel) => self.offset = self.length.wrapping_add_signed(rel)
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
	async fn async_read(&mut self, buf: &mut [u8]) -> Result<usize> {
		self.read(buf).await
	}
}

#[async_trait_fn]
impl Write<Context> for File {
	async fn async_write(&mut self, buf: &[u8]) -> Result<usize> {
		self.write(buf).await
	}

	async fn async_flush(&mut self) -> Result<()> {
		self.flush().await
	}
}

#[async_trait_fn]
impl Seek<Context> for File {
	async fn async_seek(&mut self, seek: SeekFrom) -> Result<u64> {
		self.seek(seek).await
	}

	fn stream_len_fast(&self) -> bool {
		true
	}

	async fn async_stream_len(&mut self) -> Result<u64> {
		Ok(self.length)
	}

	fn stream_position_fast(&self) -> bool {
		true
	}

	async fn async_stream_position(&mut self) -> Result<u64> {
		self.stream_position().await
	}
}

#[async_trait_fn]
impl Close<Context> for File {
	async fn async_close(self) -> Result<()> {
		self.close().await
	}
}
