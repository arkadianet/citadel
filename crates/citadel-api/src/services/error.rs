//! Service-boundary errors map to String for Tauri IPC.

pub type ServiceResult<T> = Result<T, String>;

pub fn to_string_err<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

pub trait IntoServiceError<T> {
    fn into_service(self) -> ServiceResult<T>;
}

impl<T, E: std::fmt::Display> IntoServiceError<T> for Result<T, E> {
    fn into_service(self) -> ServiceResult<T> {
        self.map_err(to_string_err)
    }
}
