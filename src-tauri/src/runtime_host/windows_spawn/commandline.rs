use std::io;

// Windows CRT argument quoting. Operate on UTF-16 so non-Unicode paths survive.
pub(super) fn encode(arguments: impl IntoIterator<Item = Vec<u16>>) -> io::Result<Vec<u16>> {
    let mut line = Vec::new();
    for arg in arguments {
        if arg.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "process argument contains NUL",
            ));
        }
        if !line.is_empty() {
            line.push(b' ' as u16);
        }
        line.push(b'"' as u16);
        let mut slashes = 0;
        for unit in arg {
            if unit == b'\\' as u16 {
                slashes += 1;
                continue;
            }
            let count = if unit == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            };
            line.extend(std::iter::repeat_n(b'\\' as u16, count));
            line.push(unit);
            slashes = 0;
        }
        line.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
        line.push(b'"' as u16);
    }
    line.push(0);
    Ok(line)
}

#[cfg(test)]
#[path = "commandline_tests.rs"]
mod tests;
