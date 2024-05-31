use super::*;

#[asynchronous]
pub trait TaskExtensions: Task + Sized {
	async fn timeout<Output>(self, duration: Duration) -> Option<Output>
	where
		Self: for<'ctx> Task<Output<'ctx> = Output>;
}

#[asynchronous]
impl<T: Task> TaskExtensions for T {
	async fn timeout<Output>(self, duration: Duration) -> Option<Output>
	where
		Self: for<'ctx> Task<Output<'ctx> = Output>
	{
		select_many! {
			res = self => Some(res),
			_ = sleep(duration) => None
		}
		.await
	}
}
