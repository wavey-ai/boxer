use access_unit::aac::ensure_adts_header;
use access_unit::{AccessUnit, PSI_STREAM_AAC, PSI_STREAM_H264};
use bytes::{Bytes, BytesMut};

const VIDEO_CODEC_H264: u8 = 7;
const VIDEO_FRAME_KEY: u8 = 1;
const AVC_SEQUENCE_HEADER: u8 = 0;
const AVC_NALU: u8 = 1;
const AUDIO_CODEC_AAC: u8 = 10;
const AAC_SEQUENCE_HEADER: u8 = 0;

#[derive(Debug)]
pub struct RtmpVideoAccessUnit {
    pub access_unit: AccessUnit,
    pub is_sequence_header: bool,
}

pub fn extract_video_access_unit(
    packet: Bytes,
    timestamp_ms: u64,
    sps_pps: Option<&Bytes>,
) -> Option<RtmpVideoAccessUnit> {
    let header = *packet.first()?;
    let frame_type = header >> 4;
    let codec = header & 0x0f;
    if codec != VIDEO_CODEC_H264 || packet.len() < 5 {
        return None;
    }

    let packet_type = packet[1];
    let composition_time = read_signed_be24(&packet[2..5]);

    match packet_type {
        AVC_SEQUENCE_HEADER => {
            let config = parse_avc_sequence_header(&packet[5..])?;
            Some(RtmpVideoAccessUnit {
                access_unit: AccessUnit {
                    stream_type: PSI_STREAM_H264,
                    key: false,
                    pts: timestamp_with_offset(timestamp_ms, composition_time),
                    dts: timestamp_ms,
                    data: config,
                    id: 0,
                },
                is_sequence_header: true,
            })
        }
        AVC_NALU => {
            let nalus = length_prefixed_to_annex_b(&packet[5..])?;
            let mut data =
                BytesMut::with_capacity(sps_pps.map_or(0, Bytes::len).saturating_add(nalus.len()));
            if let Some(sps_pps) = sps_pps {
                data.extend_from_slice(sps_pps);
            }
            data.extend_from_slice(&nalus);

            Some(RtmpVideoAccessUnit {
                access_unit: AccessUnit {
                    stream_type: PSI_STREAM_H264,
                    key: frame_type == VIDEO_FRAME_KEY,
                    pts: timestamp_with_offset(timestamp_ms, composition_time),
                    dts: timestamp_ms,
                    data: data.freeze(),
                    id: 0,
                },
                is_sequence_header: false,
            })
        }
        _ => None,
    }
}

pub fn extract_aac_access_unit(
    packet: Bytes,
    timestamp_ms: u64,
    channels: u8,
    sample_rate: u32,
    id: u64,
) -> Option<AccessUnit> {
    let raw = if packet.first()? >> 4 == AUDIO_CODEC_AAC {
        if packet.len() < 2 || packet[1] == AAC_SEQUENCE_HEADER {
            return None;
        }
        packet.slice(2..)
    } else {
        packet
    };

    if raw.is_empty() {
        return None;
    }

    Some(AccessUnit {
        stream_type: PSI_STREAM_AAC,
        key: false,
        id,
        dts: timestamp_ms,
        pts: timestamp_ms,
        data: ensure_adts_header(raw, channels, sample_rate),
    })
}

fn parse_avc_sequence_header(data: &[u8]) -> Option<Bytes> {
    if data.len() < 7 {
        return None;
    }

    let mut offset = 5;
    let sps_count = data[offset] & 0x1f;
    offset += 1;
    if sps_count == 0 {
        return None;
    }

    let mut annex_b = BytesMut::with_capacity(data.len());
    for _ in 0..sps_count {
        let sps = read_config_nalu(data, &mut offset)?;
        annex_b.extend_from_slice(&[0, 0, 0, 1]);
        annex_b.extend_from_slice(sps);
    }

    let pps_count = *data.get(offset)?;
    offset += 1;
    if pps_count == 0 {
        return None;
    }

    for _ in 0..pps_count {
        let pps = read_config_nalu(data, &mut offset)?;
        annex_b.extend_from_slice(&[0, 0, 0, 1]);
        annex_b.extend_from_slice(pps);
    }

    Some(annex_b.freeze())
}

fn read_config_nalu<'a>(data: &'a [u8], offset: &mut usize) -> Option<&'a [u8]> {
    let len = u16::from_be_bytes(data.get(*offset..*offset + 2)?.try_into().ok()?) as usize;
    *offset += 2;
    let end = offset.checked_add(len)?;
    let nalu = data.get(*offset..end)?;
    *offset = end;
    Some(nalu)
}

fn length_prefixed_to_annex_b(data: &[u8]) -> Option<Bytes> {
    let mut annex_b = BytesMut::with_capacity(data.len().saturating_add(16));
    let mut offset = 0usize;

    while offset < data.len() {
        let length_bytes = data.get(offset..offset + 4)?;
        let nalu_len = u32::from_be_bytes(length_bytes.try_into().ok()?) as usize;
        offset += 4;
        let end = offset.checked_add(nalu_len)?;
        let nalu = data.get(offset..end)?;
        offset = end;
        if nalu.is_empty() {
            continue;
        }
        annex_b.extend_from_slice(&[0, 0, 0, 1]);
        annex_b.extend_from_slice(nalu);
    }

    (!annex_b.is_empty()).then(|| annex_b.freeze())
}

fn read_signed_be24(bytes: &[u8]) -> i32 {
    let value = ((i32::from(bytes[0])) << 16) | ((i32::from(bytes[1])) << 8) | i32::from(bytes[2]);
    if value & 0x0080_0000 != 0 {
        value | !0x00ff_ffff
    } else {
        value
    }
}

fn timestamp_with_offset(timestamp_ms: u64, offset_ms: i32) -> u64 {
    if offset_ms >= 0 {
        timestamp_ms.saturating_add(offset_ms as u64)
    } else {
        timestamp_ms.saturating_sub(offset_ms.unsigned_abs().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_avc_sequence_header() {
        let packet = Bytes::from_static(&[
            0x17, 0x00, 0x00, 0x00, 0x00, // FLV AVC video tag header
            0x01, 0x42, 0x00, 0x1e, 0xff, 0xe1, 0x00, 0x04, 0x67, 0x42, 0x00, 0x1e, 0x01, 0x00,
            0x04, 0x68, 0xce, 0x06, 0xe2,
        ]);

        let video = extract_video_access_unit(packet, 123, None).expect("video au");

        assert!(video.is_sequence_header);
        assert_eq!(video.access_unit.dts, 123);
        assert_eq!(
            video.access_unit.data,
            Bytes::from_static(&[
                0, 0, 0, 1, 0x67, 0x42, 0x00, 0x1e, 0, 0, 0, 1, 0x68, 0xce, 0x06, 0xe2,
            ])
        );
    }

    #[test]
    fn parses_avc_nalu_with_signed_composition_time() {
        let packet = Bytes::from_static(&[
            0x17, 0x01, 0xff, 0xff, 0xfe, // composition time = -2
            0x00, 0x00, 0x00, 0x02, 0x65, 0x88,
        ]);

        let video = extract_video_access_unit(packet, 10, None).expect("video au");

        assert!(!video.is_sequence_header);
        assert!(video.access_unit.key);
        assert_eq!(video.access_unit.dts, 10);
        assert_eq!(video.access_unit.pts, 8);
        assert_eq!(
            video.access_unit.data,
            Bytes::from_static(&[0, 0, 0, 1, 0x65, 0x88])
        );
    }

    #[test]
    fn skips_aac_sequence_header() {
        assert!(extract_aac_access_unit(
            Bytes::from_static(&[0xaf, 0x00, 0x12, 0x10]),
            0,
            2,
            48_000,
            0
        )
        .is_none());
    }
}
