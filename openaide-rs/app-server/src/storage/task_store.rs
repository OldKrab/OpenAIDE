use std::fs;

use crate::protocol::errors::RuntimeError;

use super::atomic;
use super::records::TaskRecord;
use super::Store;

impl Store {
    pub fn write_task(&self, record: &TaskRecord) -> Result<(), RuntimeError> {
        #[cfg(test)]
        if self.take_task_write_crash_for_test() {
            panic!("injected process crash before Task record replacement");
        }
        atomic::write_json(&self.task_dir(&record.task_id)?.join("task.json"), record)
    }

    pub fn read_task(&self, task_id: &str) -> Result<TaskRecord, RuntimeError> {
        let path = self.task_dir(task_id)?.join("task.json");
        if !path.exists() {
            return Err(RuntimeError::TaskNotFound(task_id.to_string()));
        }
        let text = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn list_tasks(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        self.list_tasks_by_archived(false)
    }

    pub(crate) fn list_tasks_strict(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        self.list_tasks_by_archived_strict(false)
    }

    pub fn list_archived_tasks(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        self.list_tasks_by_archived(true)
    }

    pub(crate) fn list_archived_tasks_strict(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        self.list_tasks_by_archived_strict(true)
    }

    pub fn list_all_task_records(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        let mut records = Vec::new();
        for entry in fs::read_dir(self.tasks_dir())? {
            if let Some(record) = read_any_task_record_from_entry(entry?)? {
                records.push(record);
            }
        }
        Ok(records)
    }

    pub(crate) fn list_all_task_records_strict(&self) -> Result<Vec<TaskRecord>, RuntimeError> {
        let mut records = Vec::new();
        for entry in fs::read_dir(self.tasks_dir())? {
            if let Some(record) = read_any_task_record_from_entry_strict(entry?)? {
                records.push(record);
            }
        }
        Ok(records)
    }

    fn list_tasks_by_archived(&self, archived: bool) -> Result<Vec<TaskRecord>, RuntimeError> {
        let mut records = Vec::new();
        for entry in fs::read_dir(self.tasks_dir())? {
            if let Some(record) = read_task_record_from_entry(entry?, archived)? {
                records.push(record);
            }
        }
        records.sort_by(compare_task_records_for_navigation);
        Ok(records)
    }

    fn list_tasks_by_archived_strict(
        &self,
        archived: bool,
    ) -> Result<Vec<TaskRecord>, RuntimeError> {
        let mut records = Vec::new();
        for entry in fs::read_dir(self.tasks_dir())? {
            if let Some(record) = read_task_record_from_entry_strict(entry?, archived)? {
                records.push(record);
            }
        }
        records.sort_by(compare_task_records_for_navigation);
        Ok(records)
    }

    pub fn max_task_revision(&self) -> Result<u64, RuntimeError> {
        let mut revision = 0;
        for entry in fs::read_dir(self.tasks_dir())? {
            if let Some(record) = read_any_task_record_from_entry(entry?)? {
                revision = revision.max(record.revision);
            }
        }
        Ok(revision)
    }

    pub fn task_record_count(&self) -> Result<usize, RuntimeError> {
        let mut count = 0;
        for entry in fs::read_dir(self.tasks_dir())? {
            let entry = entry?;
            if entry.file_type()?.is_dir() && entry.path().join("task.json").exists() {
                count += 1;
            }
        }
        Ok(count)
    }
}

fn compare_task_records_for_navigation(
    left: &TaskRecord,
    right: &TaskRecord,
) -> std::cmp::Ordering {
    compare_desc(&left.last_activity, &right.last_activity)
        .then_with(|| right.last_activity.cmp(&left.last_activity))
        .then_with(|| right.task_id.cmp(&left.task_id))
}

fn compare_desc(left: &str, right: &str) -> std::cmp::Ordering {
    activity_millis(right).cmp(&activity_millis(left))
}

fn activity_millis(value: &str) -> i128 {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return 0;
    }
    if trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return trimmed.parse::<i128>().unwrap_or(0);
    }
    parse_iso_utc_millis(trimmed).unwrap_or(0)
}

fn parse_iso_utc_millis(value: &str) -> Option<i128> {
    let (date, time) = value.split_once('T')?;
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i32>().ok()?;
    let month = date_parts.next()?.parse::<u32>().ok()?;
    let day = date_parts.next()?.parse::<u32>().ok()?;
    if date_parts.next().is_some() {
        return None;
    }

    let time = time.strip_suffix('Z').unwrap_or(time);
    let mut time_parts = time.split(':');
    let hour = time_parts.next()?.parse::<u32>().ok()?;
    let minute = time_parts.next()?.parse::<u32>().ok()?;
    let second_and_millis = time_parts.next()?;
    if time_parts.next().is_some() {
        return None;
    }
    let (second, millis) = match second_and_millis.split_once('.') {
        Some((second, fraction)) => {
            let millis = fraction.chars().take(3).collect::<String>();
            let millis = format!("{millis:0<3}").parse::<u32>().ok()?;
            (second.parse::<u32>().ok()?, millis)
        }
        None => (second_and_millis.parse::<u32>().ok()?, 0),
    };
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }

    let days = days_from_civil(year, month, day);
    Some(
        (((days * 24 + hour as i128) * 60 + minute as i128) * 60 + second as i128) * 1000
            + millis as i128,
    )
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i128 {
    let year = year as i128 - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i128;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i128 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn read_task_record_from_entry(
    entry: fs::DirEntry,
    archived_filter: bool,
) -> Result<Option<TaskRecord>, RuntimeError> {
    let Some(record) = read_any_task_record_from_entry(entry)? else {
        return Ok(None);
    };
    if record.tombstoned || record.archived != archived_filter {
        return Ok(None);
    }
    Ok(Some(record))
}

fn read_task_record_from_entry_strict(
    entry: fs::DirEntry,
    archived_filter: bool,
) -> Result<Option<TaskRecord>, RuntimeError> {
    let Some(record) = read_any_task_record_from_entry_strict(entry)? else {
        return Ok(None);
    };
    if record.tombstoned || record.archived != archived_filter {
        return Ok(None);
    }
    Ok(Some(record))
}

fn read_any_task_record_from_entry(
    entry: fs::DirEntry,
) -> Result<Option<TaskRecord>, RuntimeError> {
    if !entry.file_type()?.is_dir() {
        return Ok(None);
    }
    let path = entry.path().join("task.json");
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    match serde_json::from_str(&text) {
        Ok(record) => Ok(Some(record)),
        Err(_) => Ok(None),
    }
}

fn read_any_task_record_from_entry_strict(
    entry: fs::DirEntry,
) -> Result<Option<TaskRecord>, RuntimeError> {
    if !entry.file_type()?.is_dir() {
        return Ok(None);
    }
    let path = entry.path().join("task.json");
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}
