use std::time::Duration;

use enumflags2::{bitflags, make_bitflags, BitFlags};
use xx_core::error::Result;

use crate::{
	async_runtime::*,
	driver::{Driver, TimeoutFlag},
	ops::*
};

#[bitflags]
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TimerFlag {
	Align = 1 << 0
}

pub struct Timer {
	expire: u64,
	delay: u64,
	flags: BitFlags<TimerFlag>
}

impl Timer {
	pub fn new(delay: Duration, flags: BitFlags<TimerFlag>) -> Timer {
		let delay = delay.as_nanos() as u64;
		let expire = 0;

		Timer { expire, delay, flags }
	}

	#[async_fn]
	#[inline(always)]
	pub async fn next(&mut self) -> Result<()> {
		if self.delay == 0 {
			return Ok(());
		}

		let now = Driver::now();

		self.expire = if !self.flags.intersects(TimerFlag::Align) {
			now.saturating_add(self.delay)
		} else {
			let next = self.expire.saturating_add(self.delay);

			if now <= next {
				next
			} else {
				/* slow path */
				let mut expire = now.saturating_add(self.delay);

				if self.expire != 0 {
					/* subsequent iteration, align to next delay boundary */
					let align = (now - self.expire) % self.delay;

					expire -= align;
				}

				expire
			}
		};

		timeout(self.expire, make_bitflags!(TimeoutFlag::{Abs})).await
	}
}
