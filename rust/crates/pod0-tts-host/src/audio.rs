use tokio::{
    fs::File,
    io::{AsyncReadExt as _, AsyncSeekExt as _, SeekFrom},
};

use crate::{AudioMediaType, ProtocolError, ProtocolErrorKind, TtsError, opus};

const MPEG_HEADER_BYTES: usize = 4;
const ID3_HEADER_BYTES: usize = 10;

pub(crate) async fn validate(
    file: &mut File,
    media_type: AudioMediaType,
    byte_count: u64,
    status: u16,
) -> Result<(), TtsError> {
    let valid = match media_type {
        AudioMediaType::Mpeg => valid_mpeg(file, byte_count).await?,
        AudioMediaType::Ogg => valid_ogg(file, byte_count).await?,
        AudioMediaType::Opus => opus::valid(file, byte_count).await?,
        AudioMediaType::Wav => valid_wav(file, byte_count).await?,
        AudioMediaType::Flac => valid_flac(file, byte_count).await?,
        AudioMediaType::Aac => valid_aac(file, byte_count).await?,
        AudioMediaType::Mp4 => valid_mp4(file, byte_count).await?,
        AudioMediaType::Pcm => byte_count >= 2 && byte_count.is_multiple_of(2),
        AudioMediaType::Ulaw | AudioMediaType::Alaw => byte_count > 0,
        AudioMediaType::Basic => valid_basic(file, byte_count).await?,
    };
    if valid {
        Ok(())
    } else {
        Err(TtsError::Protocol(ProtocolError {
            kind: ProtocolErrorKind::InvalidAudioPayload,
            status: Some(status),
        }))
    }
}

async fn valid_mpeg(file: &mut File, byte_count: u64) -> Result<bool, TtsError> {
    let first = read_at(file, 0, ID3_HEADER_BYTES).await?;
    let offset = if first.starts_with(b"ID3") {
        if first.len() < ID3_HEADER_BYTES || first[6..10].iter().any(|byte| byte & 0x80 != 0) {
            return Ok(false);
        }
        let tag_size = first[6..10]
            .iter()
            .fold(0_u64, |size, byte| (size << 7) | u64::from(*byte));
        ID3_HEADER_BYTES as u64 + tag_size + u64::from(first[5] & 0x10 != 0) * 10
    } else {
        0
    };
    let header = read_at(file, offset, MPEG_HEADER_BYTES).await?;
    let Some(frame_bytes) = mpeg_frame_bytes(&header) else {
        return Ok(false);
    };
    Ok(offset
        .checked_add(frame_bytes)
        .is_some_and(|end| end <= byte_count))
}

fn mpeg_frame_bytes(header: &[u8]) -> Option<u64> {
    let header = u32::from_be_bytes(header.try_into().ok()?);
    if header >> 21 != 0x7ff {
        return None;
    }
    let version = (header >> 19) & 0b11;
    let layer = (header >> 17) & 0b11;
    let bitrate_index = usize::try_from((header >> 12) & 0b1111).ok()?;
    let sample_index = usize::try_from((header >> 10) & 0b11).ok()?;
    if version == 0b01 || layer == 0 || matches!(bitrate_index, 0 | 15) || sample_index == 3 {
        return None;
    }
    let bitrate = u64::from(mpeg_bitrate(version, layer, bitrate_index)?) * 1_000;
    let base_rate = [44_100_u64, 48_000, 32_000][sample_index];
    let sample_rate = match version {
        0b11 => base_rate,
        0b10 => base_rate / 2,
        0b00 => base_rate / 4,
        _ => return None,
    };
    let padding = u64::from((header >> 9) & 1);
    let frame_bytes = if layer == 0b11 {
        ((12 * bitrate / sample_rate) + padding) * 4
    } else {
        let coefficient = if layer == 0b01 && version != 0b11 {
            72
        } else {
            144
        };
        coefficient * bitrate / sample_rate + padding
    };
    (frame_bytes >= MPEG_HEADER_BYTES as u64).then_some(frame_bytes)
}

fn mpeg_bitrate(version: u32, layer: u32, index: usize) -> Option<u16> {
    const MPEG1_LAYER1: [u16; 16] = [
        0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448, 0,
    ];
    const MPEG1_LAYER2: [u16; 16] = [
        0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 0,
    ];
    const MPEG1_LAYER3: [u16; 16] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
    ];
    const MPEG2_LAYER1: [u16; 16] = [
        0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 0,
    ];
    const MPEG2_LAYER23: [u16; 16] = [
        0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
    ];
    let table = match (version, layer) {
        (0b11, 0b11) => MPEG1_LAYER1,
        (0b11, 0b10) => MPEG1_LAYER2,
        (0b11, 0b01) => MPEG1_LAYER3,
        (_, 0b11) => MPEG2_LAYER1,
        (_, 0b10 | 0b01) => MPEG2_LAYER23,
        _ => return None,
    };
    table.get(index).copied().filter(|value| *value != 0)
}

async fn valid_ogg(file: &mut File, byte_count: u64) -> Result<bool, TtsError> {
    let header = read_at(file, 0, 27).await?;
    if header.len() < 27 || &header[..4] != b"OggS" || header[4] != 0 {
        return Ok(false);
    }
    let segment_count = usize::from(header[26]);
    let segment_table = read_at(file, 27, segment_count).await?;
    if segment_table.len() != segment_count {
        return Ok(false);
    }
    let header_bytes = 27_u64 + segment_count as u64;
    let body_bytes = segment_table
        .iter()
        .map(|value| u64::from(*value))
        .sum::<u64>();
    if body_bytes == 0
        || header_bytes
            .checked_add(body_bytes)
            .is_none_or(|end| end > byte_count)
    {
        return Ok(false);
    }
    Ok(true)
}

async fn valid_wav(file: &mut File, byte_count: u64) -> Result<bool, TtsError> {
    let header = read_at(file, 0, 12).await?;
    if header.len() != 12 || &header[..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return Ok(false);
    }
    let mut offset = 12_u64;
    let mut found_format = false;
    for _ in 0..128 {
        let chunk = read_at(file, offset, 8).await?;
        if chunk.len() != 8 {
            break;
        }
        let size = u64::from(u32::from_le_bytes(chunk[4..8].try_into().unwrap()));
        let data_start = offset + 8;
        let Some(data_end) = data_start.checked_add(size) else {
            return Ok(false);
        };
        if data_end > byte_count {
            return Ok(false);
        }
        match &chunk[..4] {
            b"fmt " => found_format = size >= 16,
            b"data" => return Ok(found_format && size > 0),
            _ => {}
        }
        offset = data_end + (size & 1);
    }
    Ok(false)
}

async fn valid_aac(file: &mut File, byte_count: u64) -> Result<bool, TtsError> {
    let header = read_at(file, 0, 7).await?;
    if header.len() != 7
        || header[0] != 0xff
        || header[1] & 0xf0 != 0xf0
        || (header[1] >> 1) & 0b11 != 0
    {
        return Ok(false);
    }
    let frame_bytes = (u64::from(header[3] & 0x03) << 11)
        | (u64::from(header[4]) << 3)
        | u64::from(header[5] >> 5);
    Ok(frame_bytes >= 7 && frame_bytes <= byte_count)
}

async fn valid_mp4(file: &mut File, byte_count: u64) -> Result<bool, TtsError> {
    let mut offset = 0_u64;
    let mut found_file_type = false;
    let mut found_metadata = false;
    let mut found_media = false;
    for _ in 0..128 {
        let header = read_at(file, offset, 8).await?;
        if header.len() != 8 {
            break;
        }
        let box_bytes = u64::from(u32::from_be_bytes(header[..4].try_into().unwrap()));
        if box_bytes < 8
            || offset
                .checked_add(box_bytes)
                .is_none_or(|end| end > byte_count)
        {
            return Ok(false);
        }
        match &header[4..8] {
            b"ftyp" => found_file_type = box_bytes >= 12,
            b"moov" => found_metadata = true,
            b"mdat" => found_media = box_bytes > 8,
            _ => {}
        }
        offset += box_bytes;
    }
    Ok(found_file_type && found_metadata && found_media)
}

async fn valid_basic(file: &mut File, byte_count: u64) -> Result<bool, TtsError> {
    let header = read_at(file, 0, 24).await?;
    if header.len() != 24 || &header[..4] != b".snd" {
        return Ok(false);
    }
    let data_offset = u64::from(u32::from_be_bytes(header[4..8].try_into().unwrap()));
    let data_bytes = u64::from(u32::from_be_bytes(header[8..12].try_into().unwrap()));
    let encoding = u32::from_be_bytes(header[12..16].try_into().unwrap());
    let available = byte_count.saturating_sub(data_offset);
    Ok(data_offset >= 24
        && data_offset < byte_count
        && encoding != 0
        && data_bytes != 0
        && (data_bytes == u64::from(u32::MAX) || data_bytes <= available))
}

async fn valid_flac(file: &mut File, byte_count: u64) -> Result<bool, TtsError> {
    let first = read_at(file, 0, 8).await?;
    if first.len() != 8 || &first[..4] != b"fLaC" || first[4] & 0x7f != 0 {
        return Ok(false);
    }
    let stream_info_bytes =
        (u64::from(first[5]) << 16) | (u64::from(first[6]) << 8) | u64::from(first[7]);
    if stream_info_bytes != 34 {
        return Ok(false);
    }
    let mut offset = 8_u64 + stream_info_bytes;
    let mut last = first[4] & 0x80 != 0;
    while !last {
        let header = read_at(file, offset, 4).await?;
        if header.len() != 4 {
            return Ok(false);
        }
        last = header[0] & 0x80 != 0;
        let block_bytes =
            (u64::from(header[1]) << 16) | (u64::from(header[2]) << 8) | u64::from(header[3]);
        let Some(end) = offset
            .checked_add(4 + block_bytes)
            .filter(|end| *end <= byte_count)
        else {
            return Ok(false);
        };
        offset = end;
    }
    Ok(offset < byte_count)
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
