use super::*;

#[asynchronous]
pub trait TaskExtensions: Task + Sized {
	async fn timeout(self, duration: Duration) -> Option<Self::Output>;
}

#[asynchronous]
impl<T: Task> TaskExtensions for T {
	async fn timeout(self, duration: Duration) -> Option<Self::Output> {
		match select(self, sleep(duration)).await {
			Select::First(res, _) => Some(res),
			Select::Second(..) => None
		}
	}
}
