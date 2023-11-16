use std::{io::SeekFrom, path::Path};

use xx_core::os::{
	fcntl::OpenFlag,
	stat::{Statx, StatxMask}
};

use super::*;

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

		if stat.mask & (StatxMask::Size as u32) != 0 {
			Ok(File {
				fd: open(path.as_ref(), OpenFlag::ReadOnly, 0).await?,
				offset: 0,
				length: stat.size
			})
		} else {
			Err(Error::new(ErrorKind::Other, "Failed to query file size"))
		}
	}

	pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
		let offset = self.offset;
		let remaining = (self.length - offset) as usize;

		read_into!(buf, remaining);

		let read = read(self.fd.as_fd(), buf, offset as i64).await?;
		let read = check_interrupt_if_zero(read).await?;

		/* store offset to prevent race condition if split and read+write called
		 * simultaneously */
		self.offset = offset + read as u64;

		Ok(read)
	}

	pub async fn write(&mut self, buf: &[u8]) -> Result<usize> {
		write_from!(buf);

		let offset = self.offset;
		let wrote = write(self.fd.as_fd(), buf, offset as i64).await?;
		let wrote = check_interrupt_if_zero(wrote).await?;

		self.offset = offset + wrote as u64;

		Ok(wrote)
	}

	pub async fn flush(&mut self) -> Result<()> {
		fsync(self.fd.as_fd()).await
	}

	pub async fn seek(&mut self, seek: SeekFrom) -> Result<u64> {
		match seek {
			SeekFrom::Start(pos) => self.offset = pos,
			SeekFrom::Current(rel) => self.offset = self.offset.checked_add_signed(rel).unwrap(),
			SeekFrom::End(rel) => self.offset = self.length.checked_add_signed(rel).unwrap()
		}

		Ok(self.offset)
	}

	pub async fn close(self) -> Result<()> {
		close(self.fd).await
	}

	pub fn pos(&self) -> u64 {
		self.offset
	}

	pub fn len(&self) -> u64 {
		self.length
	}
}

#[async_trait_impl]
impl Read for File {
	async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
		self.read(buf).await
	}
}

#[async_trait_impl]
impl Write for File {
	async fn write(&mut self, buf: &[u8]) -> Result<usize> {
		self.write(buf).await
	}

	async fn flush(&mut self) -> Result<()> {
		self.flush().await
	}
}

#[async_trait_impl]
impl Seek for File {
	async fn seek(&mut self, seek: SeekFrom) -> Result<u64> {
		self.seek(seek).await
	}

	fn stream_len_fast(&self) -> bool {
		true
	}

	async fn stream_len(&mut self) -> Result<u64> {
		Ok(self.len())
	}

	fn stream_position_fast(&self) -> bool {
		true
	}

	async fn stream_position(&mut self) -> Result<u64> {
		Ok(self.offset)
	}
}

impl Split for File {}
