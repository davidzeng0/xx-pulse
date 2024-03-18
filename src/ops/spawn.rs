use super::*;

#[asynchronous]
pub async fn spawn<T>(task: T) -> JoinHandle<T::Output>
where
	T: Task + 'static
{
	let runtime = internal_get_runtime_context().await;

	/* Safety: task is static */
	unsafe { coroutines::spawn(runtime, task) }
}
