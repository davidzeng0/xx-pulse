#![allow(clippy::unwrap_used)]

use std::{io::SeekFrom, path::Path};

use io::*;
use xx_core::os::{
	fcntl::AtFlag,
	stat::{Statx, StatxMask}
};

use super::*;

pub struct File {
	fd: OwnedFd,
	offset: u64
}

#[asynchronous]
impl File {
	#[allow(clippy::impl_trait_in_params)]
	pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
		Ok(Self {
			fd: open(path.as_ref(), BitFlags::default(), 0).await?,
			offset: 0
		})
	}

	pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
		read_into!(buf);

		let read = read(self.fd.as_fd(), buf, self.offset.try_into().unwrap()).await?;
		let read = check_interrupt_if_zero(read).await?;

		#[allow(clippy::arithmetic_side_effects)]
		(self.offset += read as u64);

		Ok(read)
	}

	pub async fn write(&mut self, buf: &[u8]) -> Result<usize> {
		write_from!(buf);

		let wrote = write(self.fd.as_fd(), buf, self.offset.try_into().unwrap()).await?;
		let wrote = check_interrupt_if_zero(wrote).await?;

		#[allow(clippy::arithmetic_side_effects)]
		(self.offset += wrote as u64);

		Ok(wrote)
	}

	pub async fn flush(&mut self) -> Result<()> {
		fsync(self.fd.as_fd()).await
	}

	pub async fn seek(&mut self, seek: SeekFrom) -> Result<u64> {
		self.offset = match seek {
			SeekFrom::Start(pos) => pos,
			SeekFrom::Current(rel) => self.offset.checked_add_signed(rel).ok_or(Core::Overflow)?,
			SeekFrom::End(rel) => self.stream_len().await?.checked_add_signed(rel).unwrap()
		};

		Ok(self.offset)
	}

	pub async fn close(self) -> Result<()> {
		close(self.fd).await
	}

	#[must_use]
	pub const fn pos(&self) -> u64 {
		self.offset
	}
}

#[asynchronous]
impl Read for File {
	async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
		self.read(buf).await
	}
}

#[asynchronous]
impl Write for File {
	async fn write(&mut self, buf: &[u8]) -> Result<usize> {
		self.write(buf).await
	}

	async fn flush(&mut self) -> Result<()> {
		self.flush().await
	}
}

#[asynchronous]
impl Seek for File {
	async fn seek(&mut self, seek: SeekFrom) -> Result<u64> {
		self.seek(seek).await
	}

	async fn stream_len(&mut self) -> Result<u64> {
		let mut stat = Statx::default();

		statx(
			Some(self.fd.as_fd()),
			"".as_ref(),
			AtFlag::EmptyPath.into(),
			BitFlags::default(),
			&mut stat
		)
		.await?;

		if stat.mask().intersects(StatxMask::Size) {
			Ok(stat.size)
		} else {
			Err(fmt_error!("Failed to query file size"))
		}
	}

	fn stream_position_fast(&self) -> bool {
		true
	}

	async fn stream_position(&mut self) -> Result<u64> {
		Ok(self.pos())
	}
}
