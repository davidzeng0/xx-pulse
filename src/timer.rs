use std::{io::Result, time::Duration};

use enumflags2::{bitflags, make_bitflags, BitFlags};

use crate::{
	async_runtime::*,
	driver::{Driver, TimeoutFlag},
	ops::timers::*
};

#[bitflags]
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TimerFlag {
	Align = 1 << 0,
	Idle  = 1 << 1
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

		let mut flags = make_bitflags!(TimeoutFlag::{Abs});

		if self.flags.intersects(TimerFlag::Idle) {
			flags |= TimeoutFlag::Idle;
		}

		timeout(self.expire, flags).await
	}
}
