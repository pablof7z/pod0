use crate::StorageError;
use crate::listening_db_codec::corrupt;

pub(crate) fn unsigned(value: i64, detail: &'static str) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| corrupt(detail))
}
pub(crate) fn optional_unsigned(
    value: Option<i64>,
    detail: &'static str,
) -> Result<Option<u64>, StorageError> {
    value.map(|value| unsigned(value, detail)).transpose()
}
pub(crate) fn count(value: i64, detail: &'static str) -> Result<u32, StorageError> {
    u32::try_from(value).map_err(|_| corrupt(detail))
}
pub(crate) fn boolean(value: i64) -> Result<bool, StorageError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(corrupt("boolean")),
    }
}
