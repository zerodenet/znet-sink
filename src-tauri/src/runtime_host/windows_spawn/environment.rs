//! Preserve the inherited UTF-16 environment, including per-drive entries.
use super::wide;
use std::{io, process::Command};
use windows_sys::Win32::{
    Globalization::{CompareStringOrdinal, CSTR_EQUAL, CSTR_GREATER_THAN, CSTR_LESS_THAN},
    System::Environment::{FreeEnvironmentStringsW, GetEnvironmentStringsW},
};

fn inherited() -> io::Result<Vec<(Vec<u16>, Vec<u16>)>> {
    struct Block(*mut u16);
    impl Drop for Block {
        fn drop(&mut self) {
            unsafe { FreeEnvironmentStringsW(self.0) };
        }
    }
    let raw = unsafe { GetEnvironmentStringsW() };
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    let block = Block(raw);
    let mut cursor = block.0;
    let mut variables = Vec::new();
    // GetEnvironmentStringsW supplies a private, double-NUL-terminated copy.
    while unsafe { *cursor } != 0 {
        let mut length = 0;
        while unsafe { *cursor.add(length) } != 0 {
            length += 1;
        }
        let entry = unsafe { std::slice::from_raw_parts(cursor, length) };
        // Search after the first unit to retain Windows' hidden '=C:' drive keys.
        let separator = entry
            .get(1..)
            .and_then(|tail| tail.iter().position(|&unit| unit == b'=' as u16))
            .map(|offset| offset + 1)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid inherited environment entry",
                )
            })?;
        let mut key = entry[..separator].to_vec();
        key.push(0);
        let mut value = entry[separator + 1..].to_vec();
        value.push(0);
        variables.push((key, value));
        cursor = unsafe { cursor.add(length + 1) };
    }
    Ok(variables)
}

pub(super) fn build(command: &Command) -> io::Result<Vec<u16>> {
    let mut variables = inherited()?;
    for (key, value) in command.get_envs() {
        let key = wide(key)?;
        if key.len() == 1 || key.contains(&(b'=' as u16)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid process environment variable name",
            ));
        }
        let mut index = 0;
        while index < variables.len() {
            if compare_names(&variables[index].0, &key)? == std::cmp::Ordering::Equal {
                variables.remove(index);
            } else {
                index += 1;
            }
        }
        if let Some(value) = value {
            variables.push((key, wide(value)?));
        }
    }
    // Windows environment blocks are sorted using case-insensitive ordinal
    // comparison, including non-ASCII variable names. Keep all values UTF-16.
    for index in 1..variables.len() {
        let mut cursor = index;
        while cursor > 0
            && compare_names(&variables[cursor - 1].0, &variables[cursor].0)?
                == std::cmp::Ordering::Greater
        {
            variables.swap(cursor - 1, cursor);
            cursor -= 1;
        }
    }
    let mut block = Vec::new();
    for (key, value) in variables {
        block.extend_from_slice(&key[..key.len() - 1]);
        block.push(b'=' as u16);
        block.extend(value);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

fn compare_names(left: &[u16], right: &[u16]) -> io::Result<std::cmp::Ordering> {
    let length = |value: &[u16]| {
        i32::try_from(value.len() - 1).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "environment name is too long")
        })
    };
    match unsafe {
        CompareStringOrdinal(
            left.as_ptr(),
            length(left)?,
            right.as_ptr(),
            length(right)?,
            1,
        )
    } {
        CSTR_EQUAL => Ok(std::cmp::Ordering::Equal),
        CSTR_LESS_THAN => Ok(std::cmp::Ordering::Less),
        CSTR_GREATER_THAN => Ok(std::cmp::Ordering::Greater),
        _ => Err(io::Error::last_os_error()),
    }
}
