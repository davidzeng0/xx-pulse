use super::*;

#[asynchronous(traitext)]
pub trait TaskExt: Task + Sized {
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

impl<T: Task> TaskExt for T {}
