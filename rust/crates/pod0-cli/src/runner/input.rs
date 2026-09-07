use std::io::BufRead;

pub(super) const MAX_INPUT_FRAME_BYTES: usize = 256 * 1024;

pub(super) enum InputFrame {
    Line(String),
    Oversized,
    InvalidUtf8,
    Eof,
}

pub(super) fn read_frame(reader: &mut impl BufRead) -> Result<InputFrame, std::io::Error> {
    let mut bytes = Vec::new();
    let mut oversized = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if bytes.is_empty() && !oversized {
                return Ok(InputFrame::Eof);
            }
            break;
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.unwrap_or(available.len());
        if !oversized {
            if bytes.len().saturating_add(count) > MAX_INPUT_FRAME_BYTES {
                oversized = true;
                bytes.clear();
            } else {
                bytes.extend_from_slice(&available[..count]);
            }
        }
        reader.consume(count + usize::from(newline.is_some()));
        if newline.is_some() {
            break;
        }
    }
    if oversized {
        return Ok(InputFrame::Oversized);
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    Ok(match String::from_utf8(bytes) {
        Ok(line) => InputFrame::Line(line),
        Err(_) => InputFrame::InvalidUtf8,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn oversized_frame_is_discarded_through_newline_before_the_next_frame() {
        let mut bytes = vec![b'x'; MAX_INPUT_FRAME_BYTES + 1];
        bytes.extend_from_slice(b"\n{\"v\":1,\"command\":\"status\"}\n");
        let mut input = Cursor::new(bytes);

        assert!(matches!(
            read_frame(&mut input).unwrap(),
            InputFrame::Oversized
        ));
        let InputFrame::Line(next) = read_frame(&mut input).unwrap() else {
            panic!("next bounded frame must remain readable")
        };
        assert_eq!(next, r#"{"v":1,"command":"status"}"#);
    }
}
