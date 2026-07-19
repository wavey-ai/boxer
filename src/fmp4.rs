use crate::mp4::{
    self, AacProfile, AudioInit, ChannelConfiguration, FragmentSample, FragmentTrack, SampleFlags,
    SamplingFrequency, VideoInit,
};
pub use crate::mp4::{AdtsHeader, AvcDecoderConfigurationRecord};
use access_unit::aac::extract_aac_data;
use access_unit::flac::{create_streaminfo, decode_frame_header};
use access_unit::{detect_audio, Fmp4};
use access_unit::{AccessUnit, AudioType};
use bytes::Bytes;

pub fn ticks_to_hz(ticks: u64, target_hz: u32) -> u64 {
    ticks
        .saturating_mul(u64::from(target_hz))
        .saturating_add(45_000)
        / 90_000
}

pub fn pts_to_ms_timescale(pts: u64) -> u64 {
    ticks_to_hz(pts, 1_000)
}

pub fn ticks_to_ms(ticks: u64) -> u64 {
    ticks_to_hz(ticks, 1_000)
}

fn u64_to_u32_saturating(value: u64) -> u32 {
    value.min(u64::from(u32::MAX)) as u32
}

fn composition_time_offset(pts: u64, dts: u64) -> i32 {
    let offset = i128::from(pts) - i128::from(dts);
    offset.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32
}

fn detect_audio_with_offset(audio_units: &[AccessUnit]) -> (AudioType, usize) {
    let Some(data) = audio_units.first().map(|unit| unit.data.as_ref()) else {
        return (AudioType::Unknown, 0);
    };

    let audio_type = detect_audio(data);
    if audio_type != AudioType::Unknown {
        return (audio_type, 0);
    }

    for offset in [12, 4] {
        if let Some(data) = data.get(offset..) {
            let audio_type = detect_audio(data);
            if audio_type != AudioType::Unknown {
                return (audio_type, offset);
            }
        }
    }

    (AudioType::Unknown, 0)
}

#[derive(Clone)]
pub struct Config {
    pub width: u16,
    pub height: u16,
    pub avcc: Option<AvcDecoderConfigurationRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcmSampleKind {
    Integer,
    Float,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcmAudioConfig {
    pub sample_rate: u32,
    pub channel_count: u16,
    pub sample_size: u8,
    pub little_endian: bool,
    pub sample_kind: PcmSampleKind,
}

/// Codec configuration for raw Opus packets stored in ISO BMFF.
///
/// Opus media always uses a 48 kHz track timescale. `input_sample_rate` is the
/// original encoder input rate recorded in `dOps`; it does not change the
/// output timescale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpusAudioConfig {
    pub input_sample_rate: u32,
    /// Logical output channels advertised by the sample entry. This is
    /// independent of each packet's internal mono/stereo coding: an Opus
    /// decoder can produce either mono or stereo output from either form.
    pub channel_count: u16,
    pub pre_skip: u16,
    pub output_gain: i16,
}

impl OpusAudioConfig {
    fn is_valid(self) -> bool {
        self.input_sample_rate > 0 && matches!(self.channel_count, 1 | 2)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioTrackConfig {
    Pcm(PcmAudioConfig),
    Opus(OpusAudioConfig),
}

pub const OPUS_OUTPUT_SAMPLE_RATE: u32 = 48_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpusPacketInfo {
    pub duration_samples: u32,
    pub encoded_channel_count: u8,
}

/// Parse and validate one raw Opus packet.
///
/// The returned duration is expressed in Opus' fixed 48 kHz output clock.
/// Payload bytes are not decoded or changed.
pub fn opus_packet_info(packet: &[u8]) -> Option<OpusPacketInfo> {
    // RFC 6716 limits each frame, rather than a multi-frame packet as a
    // whole, to 1,275 bytes. Reuse the codec's framing parser so CBR/VBR,
    // padding, frame sizes, and the 120 ms duration ceiling are all checked.
    libopus_rs::parse_packet(packet).ok()?;
    let duration_samples = u32::try_from(libopus_rs::sample_count(packet, 48_000).ok()?).ok()?;
    let encoded_channel_count = u8::try_from(libopus_rs::channels(packet).ok()?).ok()?;
    Some(OpusPacketInfo {
        duration_samples,
        encoded_channel_count,
    })
}

impl PcmAudioConfig {
    fn bytes_per_frame(self) -> Option<usize> {
        let valid_size = match self.sample_kind {
            PcmSampleKind::Integer => matches!(self.sample_size, 16 | 24 | 32),
            PcmSampleKind::Float => matches!(self.sample_size, 32 | 64),
        };
        if !valid_size || self.sample_rate == 0 || self.channel_count == 0 {
            return None;
        }
        usize::from(self.sample_size / 8).checked_mul(usize::from(self.channel_count))
    }
}

pub fn box_fmp4(
    seq: u32,
    // if None stream is audio-only
    config: Config,
    avcs: Vec<AccessUnit>,
    audio_units: Vec<AccessUnit>,
    next_dts: u64,
) -> Fmp4 {
    box_fmp4_with_init(seq, config, avcs, audio_units, next_dts, true)
}

pub fn box_fmp4_with_init(
    seq: u32,
    // if None stream is audio-only
    config: Config,
    avcs: Vec<AccessUnit>,
    audio_units: Vec<AccessUnit>,
    next_dts: u64,
    include_init: bool,
) -> Fmp4 {
    box_fmp4_with_init_and_pcm(seq, config, avcs, audio_units, next_dts, include_init, None)
}

pub fn box_fmp4_with_init_and_pcm(
    seq: u32,
    config: Config,
    avcs: Vec<AccessUnit>,
    audio_units: Vec<AccessUnit>,
    next_dts: u64,
    include_init: bool,
    pcm_config: Option<PcmAudioConfig>,
) -> Fmp4 {
    box_fmp4_with_init_and_audio_config(
        seq,
        config,
        avcs,
        audio_units,
        next_dts,
        include_init,
        pcm_config.map(AudioTrackConfig::Pcm),
    )
}

pub fn box_fmp4_with_init_and_audio_config(
    seq: u32,
    config: Config,
    avcs: Vec<AccessUnit>,
    audio_units: Vec<AccessUnit>,
    next_dts: u64,
    include_init: bool,
    audio_config: Option<AudioTrackConfig>,
) -> Fmp4 {
    let mut fmp4_data: Vec<u8> = Vec::new();
    let mut init_data: Vec<u8> = Vec::new();
    let mut total_ticks: u64 = 0;
    let mut is_key = false;
    let has_video_track = config.avcc.is_some();
    let mut avc_data = Vec::with_capacity(avcs.iter().map(|unit| unit.data.len()).sum());
    let mut audio_data: Vec<u8> =
        Vec::with_capacity(audio_units.iter().map(|unit| unit.data.len()).sum());

    let mut avc_samples = Vec::with_capacity(avcs.len());
    let mut audio_samples = Vec::with_capacity(audio_units.len());

    let mut avc_timestamps = Vec::with_capacity(avcs.len().saturating_add(1));
    let mut video_base_media_decode_time = None;

    if has_video_track && !avcs.is_empty() {
        for a in avcs.iter() {
            if a.key {
                is_key = true;
            }

            let prev_data_len = avc_data.len();
            avc_data.extend_from_slice(&a.data);
            let sample_size = (avc_data.len() - prev_data_len) as u32;
            let sample_composition_time_offset = composition_time_offset(a.pts, a.dts);

            avc_timestamps.push(a.dts);

            let flags = if a.key {
                Some(SampleFlags {
                    is_leading: 0,
                    sample_depends_on: 2,
                    sample_is_depended_on: 0,
                    sample_has_redundancy: 0,
                    sample_padding_value: 0,
                    sample_is_non_sync_sample: false,
                    sample_degradation_priority: 0,
                })
            } else {
                Some(SampleFlags {
                    is_leading: 0,
                    sample_depends_on: 1,
                    sample_is_depended_on: 0,
                    sample_has_redundancy: 0,
                    sample_padding_value: 0,
                    sample_is_non_sync_sample: true,
                    sample_degradation_priority: 0,
                })
            };

            avc_samples.push(FragmentSample {
                duration: None,
                size: Some(sample_size),
                flags,
                composition_time_offset: Some(sample_composition_time_offset),
            });
        }

        avc_timestamps.push(next_dts);
        for i in 0..avc_samples.len() {
            let duration = avc_timestamps[i + 1].saturating_sub(avc_timestamps[i]);
            total_ticks = total_ticks.saturating_add(duration);
            avc_samples[i].duration = Some(u64_to_u32_saturating(duration));
        }

        video_base_media_decode_time = avcs.first().map(|unit| unit.dts);
    } else {
        is_key = true
    }

    let audio_track_id = if has_video_track { 2 } else { 1 };

    let mut frame_info = None;
    let mut has_audio_track = false;
    let mut audio_base_media_decode_time = None;
    let mut audio_init = None;

    let (audio_type, offset) = if audio_config.is_some() {
        (AudioType::Unknown, 0)
    } else {
        detect_audio_with_offset(&audio_units)
    };

    let mut audio_ms: u32 = 0;
    let mut opus_duration_samples = 0_u64;

    match audio_config {
        Some(AudioTrackConfig::Pcm(pcm)) => {
            if let Some(bytes_per_frame) = pcm.bytes_per_frame() {
                for access_unit in &audio_units {
                    if access_unit.data.is_empty()
                        || !access_unit.data.len().is_multiple_of(bytes_per_frame)
                    {
                        continue;
                    }
                    let frame_count = access_unit.data.len() / bytes_per_frame;
                    let duration_ms = u64::try_from(frame_count)
                        .unwrap_or(u64::MAX)
                        .saturating_mul(1_000)
                        .saturating_add(u64::from(pcm.sample_rate) / 2)
                        / u64::from(pcm.sample_rate);
                    if duration_ms == 0 {
                        continue;
                    }
                    let Ok(sample_size) = u32::try_from(access_unit.data.len()) else {
                        continue;
                    };
                    let duration_ms = u64_to_u32_saturating(duration_ms);
                    audio_ms = audio_ms.saturating_add(duration_ms);
                    audio_samples.push(FragmentSample {
                        duration: Some(duration_ms),
                        size: Some(sample_size),
                        flags: None,
                        composition_time_offset: None,
                    });
                    audio_data.extend_from_slice(&access_unit.data);
                    audio_base_media_decode_time.get_or_insert(access_unit.pts);
                }
                if !audio_samples.is_empty() {
                    audio_init = Some(AudioInit::Pcm {
                        track_id: audio_track_id,
                        channel_count: pcm.channel_count,
                        sample_size: pcm.sample_size,
                        sample_rate: pcm.sample_rate,
                        little_endian: pcm.little_endian,
                        floating_point: matches!(pcm.sample_kind, PcmSampleKind::Float),
                    });
                    has_audio_track = true;
                }
            }
        }
        Some(AudioTrackConfig::Opus(opus)) if opus.is_valid() => {
            for access_unit in &audio_units {
                let Some(packet_info) = opus_packet_info(&access_unit.data) else {
                    continue;
                };
                let Ok(sample_size) = u32::try_from(access_unit.data.len()) else {
                    continue;
                };
                audio_samples.push(FragmentSample {
                    duration: Some(packet_info.duration_samples),
                    size: Some(sample_size),
                    flags: None,
                    composition_time_offset: None,
                });
                audio_data.extend_from_slice(&access_unit.data);
                opus_duration_samples =
                    opus_duration_samples.saturating_add(u64::from(packet_info.duration_samples));
                audio_base_media_decode_time.get_or_insert_with(|| {
                    access_unit
                        .pts
                        .saturating_mul(u64::from(OPUS_OUTPUT_SAMPLE_RATE) / 1_000)
                });
            }
            if !audio_samples.is_empty() {
                audio_init = Some(AudioInit::Opus {
                    track_id: audio_track_id,
                    input_sample_rate: opus.input_sample_rate,
                    channel_count: opus.channel_count,
                    pre_skip: opus.pre_skip,
                    output_gain: opus.output_gain,
                });
                has_audio_track = true;
                audio_ms = u64_to_u32_saturating(
                    opus_duration_samples
                        .saturating_mul(1_000)
                        .saturating_add(u64::from(OPUS_OUTPUT_SAMPLE_RATE) / 2)
                        / u64::from(OPUS_OUTPUT_SAMPLE_RATE),
                );
            }
        }
        Some(AudioTrackConfig::Opus(_)) => {}
        None => match audio_type {
            AudioType::Unknown => {}
            AudioType::FLAC => {
                for a in &audio_units {
                    let Some(raw_audio) = a.data.get(offset..) else {
                        continue;
                    };
                    if raw_audio.is_empty() {
                        continue;
                    }
                    let Ok(info) = decode_frame_header(raw_audio) else {
                        continue;
                    };
                    if info.sample_rate == 0 {
                        continue;
                    }
                    let frame_duration_seconds = info.block_size as f64 / info.sample_rate as f64;
                    let frame_duration_ms = (frame_duration_seconds * 1000.0).round() as u32;
                    audio_ms += frame_duration_ms;
                    audio_samples.push(FragmentSample {
                        duration: Some(frame_duration_ms),
                        size: Some(raw_audio.len() as u32),
                        flags: None,
                        composition_time_offset: None,
                    });
                    audio_data.extend_from_slice(raw_audio);
                    audio_base_media_decode_time.get_or_insert(a.pts);

                    if frame_info.is_none() {
                        frame_info = Some(info);
                    }
                }

                if !audio_samples.is_empty() {
                    has_audio_track = true;
                }
            }
            AudioType::AAC => {
                let mut sampling_frequency = SamplingFrequency::Hz48000;
                let mut channel_configuration = ChannelConfiguration::TwoChannels;
                let mut profile = AacProfile::Main;

                for a in audio_units.iter() {
                    if let Some(header) = AdtsHeader::read_from(&a.data) {
                        let Some(frame) = extract_aac_data(&a.data) else {
                            continue;
                        };
                        let sample_size = frame.len() as u32;
                        sampling_frequency = header.sampling_frequency;
                        channel_configuration = header.channel_configuration;
                        profile = header.profile;
                        let frame_duration: u32 =
                            (1024.0 / sampling_frequency.as_u32() as f32 * 1000.0).round() as u32;
                        audio_ms += frame_duration;
                        audio_samples.push(FragmentSample {
                            duration: Some(frame_duration),
                            size: Some(sample_size),
                            flags: None,
                            composition_time_offset: None,
                        });
                        audio_data.extend_from_slice(&frame);
                        audio_base_media_decode_time.get_or_insert(a.pts);
                    }
                }

                if !audio_samples.is_empty() {
                    audio_init = Some(AudioInit::Aac {
                        track_id: audio_track_id,
                        profile,
                        frequency: sampling_frequency,
                        channel_configuration,
                    });
                    has_audio_track = true;
                }
            }
            _ => {}
        },
    }

    if matches!(audio_type, AudioType::FLAC) {
        if let Some(frame_info) = frame_info {
            audio_init = Some(AudioInit::Flac {
                track_id: audio_track_id,
                channel_count: frame_info.channels.into(),
                sample_size: frame_info.bps.into(),
                sample_rate: frame_info.sample_rate,
                streaminfo: create_streaminfo(&frame_info),
            });
        }
    }

    let mut tracks = Vec::with_capacity(2);
    if !avc_data.is_empty() && !avc_samples.is_empty() {
        tracks.push(FragmentTrack {
            track_id: 1,
            base_media_decode_time: video_base_media_decode_time.unwrap_or(0),
            samples: avc_samples,
            data: &avc_data,
        });
    }
    if has_audio_track && !audio_data.is_empty() && !audio_samples.is_empty() {
        tracks.push(FragmentTrack {
            track_id: audio_track_id,
            base_media_decode_time: audio_base_media_decode_time.unwrap_or(0),
            samples: audio_samples,
            data: &audio_data,
        });
    }
    let _ = mp4::write_media_segment(&mut fmp4_data, seq, &tracks);

    if include_init {
        let video_init = config.avcc.as_ref().map(|avcc| VideoInit {
            track_id: 1,
            width: config.width,
            height: config.height,
            avcc: avcc.clone(),
        });
        let movie_timescale = if has_video_track {
            90_000
        } else {
            audio_init
                .as_ref()
                .map(AudioInit::timescale)
                .unwrap_or(1_000)
        };
        let _ = mp4::write_init_segment(
            &mut init_data,
            movie_timescale,
            video_init.as_ref(),
            audio_init.as_ref(),
        );
    }

    let mut init: Option<Bytes> = None;
    if !init_data.is_empty() {
        init = Some(Bytes::from(init_data))
    }

    Fmp4 {
        init,
        duration: if avcs.is_empty() {
            audio_ms
        } else {
            ticks_to_ms(total_ticks) as u32
        },
        key: is_key,
        data: Bytes::from(fmp4_data),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use access_unit::PSI_STREAM_H264;

    fn read_u16(bytes: &[u8]) -> u16 {
        u16::from_be_bytes([bytes[0], bytes[1]])
    }

    fn read_u32(bytes: &[u8]) -> u32 {
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    fn read_u64(bytes: &[u8]) -> u64 {
        u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ])
    }

    fn box_type_offsets(data: &[u8], box_type: &[u8; 4]) -> Vec<usize> {
        data.windows(4)
            .enumerate()
            .filter_map(|(offset, window)| (window == box_type).then_some(offset))
            .collect()
    }

    fn full_box_u32_values(data: &[u8], box_type: &[u8; 4]) -> Vec<u32> {
        box_type_offsets(data, box_type)
            .into_iter()
            .filter_map(|offset| data.get(offset + 8..offset + 12))
            .map(read_u32)
            .collect()
    }

    fn box_payload_len(data: &[u8], box_type: &[u8; 4]) -> Option<usize> {
        let offset = box_type_offsets(data, box_type).into_iter().next()?;
        let size_offset = offset.checked_sub(4)?;
        let size = read_u32(data.get(size_offset..size_offset + 4)?) as usize;
        size.checked_sub(8)
    }

    fn box_payload<'a>(data: &'a [u8], box_type: &[u8; 4]) -> Option<&'a [u8]> {
        let type_offset = box_type_offsets(data, box_type).into_iter().next()?;
        let start = type_offset.checked_sub(4)?;
        let size = read_u32(data.get(start..start + 4)?) as usize;
        data.get(type_offset + 4..start.checked_add(size)?)
    }

    fn config() -> Config {
        Config {
            width: 1920,
            height: 1080,
            avcc: Some(AvcDecoderConfigurationRecord {
                profile_idc: 66,
                constraint_set_flag: 0,
                level_idc: 30,
                sequence_parameter_set: Bytes::from_static(&[0x67, 0x42, 0x00, 0x1e]),
                picture_parameter_set: Bytes::from_static(&[0x68, 0xce, 0x06, 0xe2]),
            }),
        }
    }

    fn video_unit(dts: u64, pts: u64, key: bool) -> AccessUnit {
        AccessUnit {
            key,
            pts,
            dts,
            data: Bytes::from_static(&[0, 0, 0, 1, 0x65]),
            stream_type: PSI_STREAM_H264,
            id: 0,
        }
    }

    fn flac_unit() -> AccessUnit {
        let data =
            std::fs::read("../access-unit/testdata/flac/A_Tusk_is_used_to_make_costly_gifts.flac")
                .expect("read FLAC fixture");
        let frame = access_unit::flac::extract_flac_frame(&data);
        assert!(!frame.is_empty());
        AccessUnit {
            key: true,
            pts: 0,
            dts: 0,
            data: Bytes::copy_from_slice(frame),
            stream_type: 0,
            id: 1,
        }
    }

    fn aac_unit() -> AccessUnit {
        let payload = [0x11, 0x22, 0x33, 0x44];
        let mut data = access_unit::aac::create_adts_header(0x66, 2, 48_000, payload.len(), false);
        data.extend_from_slice(&payload);
        AccessUnit {
            key: true,
            pts: 0,
            dts: 0,
            data: Bytes::from(data),
            stream_type: 0,
            id: 1,
        }
    }

    #[test]
    fn high_video_decode_time_uses_tfdt_version_one() {
        let dts = u64::from(u32::MAX) + 90_000;
        let fmp4 = box_fmp4(
            1,
            config(),
            vec![video_unit(dts, dts, true)],
            Vec::new(),
            dts + 3_000,
        );
        let tfdt = box_type_offsets(&fmp4.data, b"tfdt")
            .into_iter()
            .next()
            .expect("tfdt box");

        assert_eq!(fmp4.data[tfdt + 4], 1);
        assert_eq!(read_u64(&fmp4.data[tfdt + 8..tfdt + 16]), dts);
    }

    #[test]
    fn video_and_flac_fragment_keeps_video_data_and_audio_track_two() {
        let video = video_unit(90_000, 90_000, true);
        let video_len = video.data.len();
        let flac = flac_unit();
        let flac_len = flac.data.len();

        let fmp4 = box_fmp4(2, config(), vec![video], vec![flac], 93_000);

        assert_eq!(full_box_u32_values(&fmp4.data, b"tfhd"), vec![1, 2]);
        assert_eq!(
            box_payload_len(&fmp4.data, b"mdat"),
            Some(video_len + flac_len)
        );
    }

    #[test]
    fn audio_only_fragment_with_video_config_uses_audio_track_two() {
        let fmp4 = box_fmp4(3, config(), Vec::new(), vec![flac_unit()], 0);

        assert_eq!(full_box_u32_values(&fmp4.data, b"tfhd"), vec![2]);
    }

    #[test]
    fn aac_audio_only_writes_init_and_payload() {
        let aac = aac_unit();
        let payload_len = access_unit::aac::extract_aac_data(&aac.data)
            .expect("aac payload")
            .len();

        let fmp4 = box_fmp4(
            4,
            Config {
                width: 0,
                height: 0,
                avcc: None,
            },
            Vec::new(),
            vec![aac],
            0,
        );
        let init = fmp4.init.as_ref().expect("init segment");

        assert_eq!(full_box_u32_values(&fmp4.data, b"tfhd"), vec![1]);
        assert_eq!(box_payload_len(&fmp4.data, b"mdat"), Some(payload_len));
        assert!(!box_type_offsets(init, b"mp4a").is_empty());
        assert!(!box_type_offsets(init, b"esds").is_empty());
    }

    #[test]
    fn box_fmp4_with_init_can_skip_init_segment() {
        let fmp4 = box_fmp4_with_init(
            5,
            config(),
            vec![video_unit(0, 0, true)],
            Vec::new(),
            3_000,
            false,
        );

        assert!(fmp4.init.is_none());
        assert!(!fmp4.data.is_empty());
    }

    #[test]
    fn short_unknown_audio_does_not_panic() {
        let fmp4 = box_fmp4(
            6,
            Config {
                width: 0,
                height: 0,
                avcc: None,
            },
            Vec::new(),
            vec![AccessUnit {
                key: true,
                pts: 0,
                dts: 0,
                data: Bytes::from_static(&[1, 2, 3]),
                stream_type: 0,
                id: 2,
            }],
            0,
        );

        assert!(fmp4.data.is_empty());
    }

    #[test]
    fn integer_pcm_fmp4_preserves_exact_s24le_samples_and_signals_format() {
        let pcm: Vec<u8> = (0..240 * 2 * 3)
            .map(|index| ((index * 37 + 11) & 0xff) as u8)
            .collect();
        let fmp4 = box_fmp4_with_init_and_pcm(
            7,
            Config {
                width: 0,
                height: 0,
                avcc: None,
            },
            Vec::new(),
            vec![AccessUnit {
                key: true,
                pts: 0,
                dts: 0,
                data: Bytes::copy_from_slice(&pcm),
                stream_type: 0,
                id: 3,
            }],
            0,
            true,
            Some(PcmAudioConfig {
                sample_rate: 48_000,
                channel_count: 2,
                sample_size: 24,
                little_endian: true,
                sample_kind: PcmSampleKind::Integer,
            }),
        );
        let init = fmp4.init.as_ref().expect("PCM init segment");
        let mdat = box_type_offsets(&fmp4.data, b"mdat")
            .into_iter()
            .next()
            .expect("mdat box");
        let pcmc = box_type_offsets(init, b"pcmC")
            .into_iter()
            .next()
            .expect("pcmC box");
        let chnl = box_type_offsets(init, b"chnl")
            .into_iter()
            .next()
            .expect("chnl box");

        assert_eq!(fmp4.duration, 5);
        assert_eq!(&fmp4.data[mdat + 4..], pcm);
        assert!(!box_type_offsets(init, b"ipcm").is_empty());
        assert_eq!(&init[pcmc + 8..pcmc + 10], &[1, 24]);
        assert_eq!(&init[chnl + 8..chnl + 12], &[1, 0, 127, 127]);
    }

    #[test]
    fn floating_point_pcm_uses_fpcm_sample_entry() {
        let fmp4 = box_fmp4_with_init_and_pcm(
            8,
            Config {
                width: 0,
                height: 0,
                avcc: None,
            },
            Vec::new(),
            vec![AccessUnit {
                key: true,
                pts: 0,
                dts: 0,
                data: Bytes::from(vec![0; 240 * 4]),
                stream_type: 0,
                id: 4,
            }],
            0,
            true,
            Some(PcmAudioConfig {
                sample_rate: 48_000,
                channel_count: 1,
                sample_size: 32,
                little_endian: true,
                sample_kind: PcmSampleKind::Float,
            }),
        );
        let init = fmp4.init.as_ref().expect("PCM init segment");

        assert!(!box_type_offsets(init, b"fpcm").is_empty());
        assert!(box_type_offsets(init, b"ipcm").is_empty());
    }

    #[test]
    fn raw_opus_mono_and_stereo_use_dops_and_preserve_5ms_packets_exactly() {
        for channel_count in [1_u16, 2] {
            // CELT-only configuration 17 is 5 ms at the fixed 48 kHz Opus
            // output rate. Code zero carries one frame in each packet. A
            // stereo output may legally carry an adaptively mono-coded packet.
            let mono_toc = 17 << 3;
            let output_toc = mono_toc | (u8::from(channel_count == 2) << 2);
            let first = vec![mono_toc, 0x11, 0x22];
            let second = vec![output_toc, 0x33];
            let units = [first.clone(), second.clone()]
                .into_iter()
                .enumerate()
                .map(|(index, packet)| AccessUnit {
                    key: true,
                    pts: 10 + index as u64 * 5,
                    dts: 10 + index as u64 * 5,
                    data: Bytes::from(packet),
                    stream_type: access_unit::PSI_STREAM_AUDIO_OPUS,
                    id: index as u64,
                })
                .collect();

            let fmp4 = box_fmp4_with_init_and_audio_config(
                9,
                Config {
                    width: 0,
                    height: 0,
                    avcc: None,
                },
                Vec::new(),
                units,
                0,
                true,
                Some(AudioTrackConfig::Opus(OpusAudioConfig {
                    input_sample_rate: 48_000,
                    channel_count,
                    pre_skip: 0,
                    output_gain: 0,
                })),
            );
            let init = fmp4.init.as_ref().expect("Opus init segment");
            let opus = box_type_offsets(init, b"Opus")[0];
            let dops = box_payload(init, b"dOps").expect("dOps payload");
            let mdhd = box_type_offsets(init, b"mdhd")[0];
            let trun = box_type_offsets(&fmp4.data, b"trun")[0];
            let tfdt = box_type_offsets(&fmp4.data, b"tfdt")[0];
            let mut expected_payload = first;
            expected_payload.extend_from_slice(&second);

            assert_eq!(fmp4.duration, 10);
            assert_eq!(
                box_payload(&fmp4.data, b"mdat"),
                Some(expected_payload.as_slice())
            );
            assert_eq!(read_u16(&init[opus + 20..opus + 22]), channel_count);
            assert_eq!(read_u32(&init[opus + 28..opus + 32]), 48_000 << 16);
            assert_eq!(read_u32(&init[mdhd + 16..mdhd + 20]), 48_000);
            assert_eq!(
                dops,
                &[0, channel_count as u8, 0, 0, 0, 0, 0xbb, 0x80, 0, 0, 0]
            );
            assert_eq!(read_u32(&fmp4.data[tfdt + 8..tfdt + 12]), 480);
            assert_eq!(read_u32(&fmp4.data[trun + 8..trun + 12]), 2);
            assert_eq!(read_u32(&fmp4.data[trun + 16..trun + 20]), 240);
            assert_eq!(
                read_u32(&fmp4.data[trun + 20..trun + 24]),
                expected_payload.len() as u32 - second.len() as u32
            );
            assert_eq!(read_u32(&fmp4.data[trun + 24..trun + 28]), 240);
            assert_eq!(
                read_u32(&fmp4.data[trun + 28..trun + 32]),
                second.len() as u32
            );
        }
    }

    #[test]
    fn opus_packet_info_accepts_tiny_valid_packets_and_rejects_invalid_durations() {
        assert_eq!(
            opus_packet_info(&[17 << 3]),
            Some(OpusPacketInfo {
                duration_samples: 240,
                encoded_channel_count: 1,
            })
        );
        assert_eq!(
            opus_packet_info(&[(17 << 3) | (1 << 2)]),
            Some(OpusPacketInfo {
                duration_samples: 240,
                encoded_channel_count: 2,
            })
        );
        assert_eq!(opus_packet_info(&[]), None);
        assert_eq!(opus_packet_info(&[(3 << 3) | 3, 3]), None);
        // A code-zero packet has one frame after the TOC. A 1,275-byte frame
        // is valid; 1,276 bytes is not.
        assert!(opus_packet_info(&vec![0; 1_276]).is_some());
        assert_eq!(opus_packet_info(&vec![0; 1_277]), None);
    }

    #[test]
    fn opus_multiframe_packet_larger_than_1275_bytes_is_valid_and_preserved() {
        // Code one: two equal-sized CBR frames. Each 1,000-byte frame is under
        // RFC 6716's per-frame ceiling even though the packet is 2,001 bytes.
        let mut packet = vec![(17 << 3) | 1];
        packet.extend((0..2_000).map(|index| index as u8));
        assert_eq!(packet.len(), 2_001);
        assert_eq!(
            opus_packet_info(&packet),
            Some(OpusPacketInfo {
                duration_samples: 480,
                encoded_channel_count: 1,
            })
        );

        let fmp4 = box_fmp4_with_init_and_audio_config(
            1,
            Config {
                width: 0,
                height: 0,
                avcc: None,
            },
            Vec::new(),
            vec![AccessUnit {
                key: true,
                pts: 0,
                dts: 0,
                data: Bytes::from(packet.clone()),
                stream_type: access_unit::PSI_STREAM_AUDIO_OPUS,
                id: 0,
            }],
            0,
            true,
            Some(AudioTrackConfig::Opus(OpusAudioConfig {
                input_sample_rate: 48_000,
                channel_count: 1,
                pre_skip: 0,
                output_gain: 0,
            })),
        );

        assert_eq!(fmp4.duration, 10);
        assert_eq!(box_payload(&fmp4.data, b"mdat"), Some(packet.as_slice()));
        let trun = box_type_offsets(&fmp4.data, b"trun")[0];
        assert_eq!(read_u32(&fmp4.data[trun + 16..trun + 20]), 480);
    }

    #[test]
    fn mono_opus_track_accepts_stereo_coded_packet_for_decoder_downmix() {
        let packet = [(17 << 3) | (1 << 2)];
        let fmp4 = box_fmp4_with_init_and_audio_config(
            1,
            Config {
                width: 0,
                height: 0,
                avcc: None,
            },
            Vec::new(),
            vec![AccessUnit {
                key: true,
                pts: 0,
                dts: 0,
                data: Bytes::copy_from_slice(&packet),
                stream_type: access_unit::PSI_STREAM_AUDIO_OPUS,
                id: 0,
            }],
            0,
            true,
            Some(AudioTrackConfig::Opus(OpusAudioConfig {
                input_sample_rate: 48_000,
                channel_count: 1,
                pre_skip: 0,
                output_gain: 0,
            })),
        );

        let init = fmp4.init.expect("mono Opus init");
        let opus = box_type_offsets(&init, b"Opus")[0];
        assert_eq!(read_u16(&init[opus + 20..opus + 22]), 1);
        assert_eq!(box_payload(&fmp4.data, b"mdat"), Some(packet.as_slice()));
        assert_eq!(fmp4.duration, 5);
    }
}
