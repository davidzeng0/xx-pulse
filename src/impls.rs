use super::*;

#[asynchronous]
pub trait TaskExtensions: Task + Sized {
	async fn timeout(self, duration: Duration) -> Option<Self::Output>;
}

#[asynchronous]
impl<T: Task> TaskExtensions for T {
	async fn timeout(self, duration: Duration) -> Option<Self::Output> {
		select_many! {
			res = self => Some(res),
			_ = sleep(duration) => None
		}
		.await
	}
}
