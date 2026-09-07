use tokio::{
    fs::File,
    io::{AsyncReadExt as _, AsyncSeekExt as _, SeekFrom},
};

use crate::TtsError;

const OGG_HEADER_BYTES: usize = 27;
const MAX_OPUS_PACKET_BYTES: usize = 61_440;

pub(crate) async fn valid(file: &mut File, byte_count: u64) -> Result<bool, TtsError> {
    let Some(first) = read_page(file, 0, byte_count).await? else {
        return Ok(false);
    };
    if first.header_type & 0x02 == 0
        || first.header_type & 0x01 != 0
        || first.sequence != 0
        || first.granule != 0
        || first.segments.is_empty()
        || first.segments.last() == Some(&255)
        || first
            .segments
            .iter()
            .filter(|length| **length < 255)
            .count()
            != 1
        || opus_head_stream_count(&first.body).is_none()
    {
        return Ok(false);
    }

    let mut offset = first.next_offset;
    let serial = first.serial;
    let mut expected_sequence = 1_u32;
    let mut packet = Vec::new();
    let mut allow_comment = true;

    while offset < byte_count {
        let Some(page) = read_page(file, offset, byte_count).await? else {
            return Ok(false);
        };
        if page.serial != serial
            || page.sequence != expected_sequence
            || page.header_type & 0x02 != 0
            || (page.header_type & 0x01 != 0) == packet.is_empty()
        {
            return Ok(false);
        }

        let mut body_offset = 0_usize;
        for length in &page.segments {
            let length = usize::from(*length);
            let Some(segment_end) = body_offset.checked_add(length) else {
                return Ok(false);
            };
            let Some(segment) = page.body.get(body_offset..segment_end) else {
                return Ok(false);
            };
            if packet
                .len()
                .checked_add(length)
                .is_none_or(|size| size > MAX_OPUS_PACKET_BYTES)
            {
                return Ok(false);
            }
            packet.extend_from_slice(segment);
            body_offset = segment_end;

            if length < 255 {
                if allow_comment && packet.starts_with(b"OpusTags") {
                    allow_comment = false;
                } else {
                    return Ok(valid_opus_packet(&packet));
                }
                packet.clear();
            }
        }

        if page.header_type & 0x04 != 0 {
            return Ok(false);
        }
        offset = page.next_offset;
        expected_sequence = expected_sequence.wrapping_add(1);
    }
    Ok(false)
}

struct OggPage {
    header_type: u8,
    granule: u64,
    serial: u32,
    sequence: u32,
    segments: Vec<u8>,
    body: Vec<u8>,
    next_offset: u64,
}

async fn read_page(
    file: &mut File,
    offset: u64,
    byte_count: u64,
) -> Result<Option<OggPage>, TtsError> {
    let header = read_at(file, offset, OGG_HEADER_BYTES).await?;
    if header.len() != OGG_HEADER_BYTES
        || &header[..4] != b"OggS"
        || header[4] != 0
        || header[5] & !0x07 != 0
    {
        return Ok(None);
    }
    let segment_count = usize::from(header[26]);
    let table_offset = offset + OGG_HEADER_BYTES as u64;
    let segments = read_at(file, table_offset, segment_count).await?;
    if segments.len() != segment_count {
        return Ok(None);
    }
    let body_bytes = segments.iter().map(|value| u64::from(*value)).sum();
    let Some(body_offset) = table_offset.checked_add(segment_count as u64) else {
        return Ok(None);
    };
    let Some(next_offset) = body_offset
        .checked_add(body_bytes)
        .filter(|end| *end <= byte_count)
    else {
        return Ok(None);
    };
    let body = read_at(file, body_offset, body_bytes as usize).await?;
    if body.len() as u64 != body_bytes {
        return Ok(None);
    }
    Ok(Some(OggPage {
        header_type: header[5],
        granule: u64::from_le_bytes(header[6..14].try_into().unwrap()),
        serial: u32::from_le_bytes(header[14..18].try_into().unwrap()),
        sequence: u32::from_le_bytes(header[18..22].try_into().unwrap()),
        segments,
        body,
        next_offset,
    }))
}

fn opus_head_stream_count(packet: &[u8]) -> Option<u8> {
    if packet.len() < 19 || &packet[..8] != b"OpusHead" || !(1..=15).contains(&packet[8]) {
        return None;
    }
    let channels = packet[9];
    if channels == 0 {
        return None;
    }
    let family = packet[18];
    let expected_length = if family == 0 {
        if channels > 2 {
            return None;
        }
        19
    } else {
        if family == 1 && channels > 8 {
            return None;
        }
        let length = 21 + usize::from(channels);
        if packet.len() < length {
            return None;
        }
        let streams = packet[19];
        let coupled = packet[20];
        let decoded_channels = streams.checked_add(coupled)?;
        if streams == 0
            || coupled > streams
            || packet[21..length]
                .iter()
                .any(|mapping| *mapping != 255 && *mapping >= decoded_channels)
        {
            return None;
        }
        length
    };
    if packet.len() < expected_length || (packet[8] == 1 && packet.len() != expected_length) {
        return None;
    }
    Some(if family == 0 { 1 } else { packet[19] })
}

fn valid_opus_packet(packet: &[u8]) -> bool {
    if packet.is_empty() || packet.len() > MAX_OPUS_PACKET_BYTES {
        return false;
    }
    let config = packet[0] >> 3;
    let duration_units = match config {
        0..=11 => [4, 8, 16, 24][usize::from(config & 3)],
        12..=15 => [4, 8][usize::from(config & 1)],
        _ => [1, 2, 4, 8][usize::from(config & 3)],
    };
    match packet[0] & 3 {
        0 => packet.len() - 1 <= 1_275,
        1 => (packet.len() - 1).is_multiple_of(2) && (packet.len() - 1) / 2 <= 1_275,
        2 => valid_two_frame_vbr(packet),
        3 => valid_signalled_frames(packet, duration_units),
        _ => unreachable!(),
    }
}

fn valid_two_frame_vbr(packet: &[u8]) -> bool {
    let Some((first_length, length_bytes)) = decode_frame_length(&packet[1..]) else {
        return false;
    };
    let remaining = packet.len() - 1 - length_bytes;
    first_length <= remaining && first_length <= 1_275 && remaining - first_length <= 1_275
}

fn valid_signalled_frames(packet: &[u8], duration_units: usize) -> bool {
    let Some(&frame_count_byte) = packet.get(1) else {
        return false;
    };
    let frame_count = usize::from(frame_count_byte & 0x3f);
    if frame_count == 0 || frame_count * duration_units > 48 {
        return false;
    }

    let mut cursor = 2;
    let mut padding = 0_usize;
    if frame_count_byte & 0x40 != 0 {
        loop {
            let Some(&value) = packet.get(cursor) else {
                return false;
            };
            cursor += 1;
            padding = match padding.checked_add(if value == 255 {
                254
            } else {
                usize::from(value)
            }) {
                Some(padding) => padding,
                None => return false,
            };
            if value != 255 {
                break;
            }
        }
    }
    let Some(data_end) = packet
        .len()
        .checked_sub(padding)
        .filter(|end| *end >= cursor)
    else {
        return false;
    };

    if frame_count_byte & 0x80 == 0 {
        let data_bytes = data_end - cursor;
        return data_bytes.is_multiple_of(frame_count) && data_bytes / frame_count <= 1_275;
    }

    let mut declared_bytes = 0_usize;
    for _ in 1..frame_count {
        let Some((frame_bytes, length_bytes)) = decode_frame_length(&packet[cursor..data_end])
        else {
            return false;
        };
        if frame_bytes > 1_275 {
            return false;
        }
        cursor += length_bytes;
        declared_bytes += frame_bytes;
    }
    let remaining = data_end - cursor;
    declared_bytes <= remaining && remaining - declared_bytes <= 1_275
}

fn decode_frame_length(bytes: &[u8]) -> Option<(usize, usize)> {
    let first = usize::from(*bytes.first()?);
    if first < 252 {
        Some((first, 1))
    } else {
        Some((first + usize::from(*bytes.get(1)?) * 4, 2))
    }
}

async fn read_at(file: &mut File, offset: u64, length: usize) -> Result<Vec<u8>, TtsError> {
    file.seek(SeekFrom::Start(offset))
        .await
        .map_err(|error| TtsError::from_io(crate::FileOperation::ReadTemporary, &error))?;
    let mut bytes = vec![0; length];
    let mut read = 0;
    while read < length {
        let count = file
            .read(&mut bytes[read..])
            .await
            .map_err(|error| TtsError::from_io(crate::FileOperation::ReadTemporary, &error))?;
        if count == 0 {
            break;
        }
        read += count;
    }
    bytes.truncate(read);
    Ok(bytes)
}
