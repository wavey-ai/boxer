use access_unit::aac::extract_aac_data;
use access_unit::flac::{create_streaminfo, decode_frame_header};
use access_unit::{detect_audio, Fmp4};
use access_unit::{AccessUnit, AudioType};
use bytes::Bytes;
use mse_fmp4::io::WriteTo;
use mse_fmp4::{
    aac::{AacProfile, AdtsHeader, ChannelConfiguration, SamplingFrequency},
    avc::AvcDecoderConfigurationRecord,
    flac::{FLACMetadataBlock, FLACSpecificBox},
    fmp4::{
        AacSampleEntry, AvcConfigurationBox, AvcSampleEntry, FLACSampleEntry,
        InitializationSegment, MediaSegment, MovieExtendsHeaderBox, Mpeg4EsDescriptorBox, Sample,
        SampleEntry, SampleFlags, TrackBox, TrackExtendsBox, TrackFragmentBox, TrackType,
    },
};
use tracing::{error, info};

pub fn ticks_to_hz(ticks: u64, target_hz: u32) -> u64 {
    (ticks * u64::from(target_hz) + 45000) / 90000
}

pub fn pts_to_ms_timescale(pts: u64) -> u64 {
    // Convert from 90kHz to 1000Hz (milliseconds)
    (pts * 1000 + 45000) / 90000
}

pub fn ticks_to_ms(ticks: u64) -> u64 {
    // Convert ticks to seconds as f64
    let seconds = ticks as f64 / 90000.0;

    // Convert seconds to milliseconds
    (seconds * 1000.0) as u64
}

pub fn box_fmp4(
    seq: u32,
    // if None stream is audio-only
    avcc: Option<&AvcDecoderConfigurationRecord>,
    avcs: Vec<AccessUnit>,
    audio_units: Vec<AccessUnit>,
    next_dts: u64,
) -> Fmp4 {
    let mut segment = MediaSegment::new(seq);
    let mut fmp4_data: Vec<u8> = Vec::new();
    let mut init_data: Vec<u8> = Vec::new();
    let mut total_ticks = 0;
    let mut is_key = false;
    let mut avc_data = Vec::new();
    let mut audio_data: Vec<u8> = Vec::new();

    let mut avc_samples = Vec::new();
    let mut audio_samples = Vec::new();

    if avcc.is_some() {
        let mut avc_timestamps = Vec::new();

        for a in avcs.iter() {
            if a.key {
                is_key = true;
            }

            let prev_data_len = &avc_data.len();
            avc_data.extend_from_slice(&a.data);
            let sample_size = (avc_data.len() - prev_data_len) as u32;
            let sample_composition_time_offset = (a.pts - a.dts) as i32;

            avc_timestamps.push(a.dts);

            let flags = if a.key {
                Some(SampleFlags {
                    is_leading: 0,
                    sample_depends_on: 0,
                    sample_is_depdended_on: 0,
                    sample_has_redundancy: 0,
                    sample_padding_value: 0,
                    sample_is_non_sync_sample: false,
                    sample_degradation_priority: 0,
                })
            } else {
                Some(SampleFlags {
                    is_leading: 0,
                    sample_depends_on: 1,
                    sample_is_depdended_on: 0,
                    sample_has_redundancy: 0,
                    sample_padding_value: 0,
                    sample_is_non_sync_sample: true,
                    sample_degradation_priority: 0,
                })
            };

            avc_samples.push(Sample {
                duration: None,
                size: Some(sample_size),
                flags,
                composition_time_offset: Some(sample_composition_time_offset),
            });
        }

        avc_timestamps.push(next_dts);
        for i in 0..avc_samples.len() {
            let duration = avc_timestamps[i + 1] - avc_timestamps[i];
            total_ticks += duration;
            avc_samples[i].duration = Some(duration as u32);
        }

        let mut traf = TrackFragmentBox::new(1);
        traf.trun_box.first_sample_flags = None;
        traf.tfhd_box.default_sample_flags = None;
        traf.trun_box.data_offset = Some(0);
        traf.trun_box.samples = avc_samples;
        traf.tfdt_box.base_media_decode_time = avcs[0].dts as u32;
        segment.moof_box.traf_boxes.push(traf);
    } else {
        is_key = true
    }

    let audio_track_id = if avc_data.len() > 0 { 2 } else { 1 };

    let mut audio_track = TrackBox::new(audio_track_id, false);
    let mut frame_info = None;

    let audio_type = if audio_units.len() > 0 {
        detect_audio(&audio_units[0].data);
        AudioType::AAC
    } else {
        AudioType::Unkownn
    };

    match audio_type {
        AudioType::Unkownn => {
            if avc_data.len() > 0 {
                segment.add_track_data(0, &avc_data);
            }
            segment.update_offsets();
            segment.write_to(&mut fmp4_data).unwrap();
        }
        AudioType::FLAC => {
            let info = decode_frame_header(&audio_units[0].data).unwrap();
            let frame_duration = info.block_size;
            frame_info = Some(info);

            for a in &audio_units {
                let raw_audio = &a.data;
                audio_samples.push(Sample {
                    duration: Some(frame_duration as u32),
                    size: Some(raw_audio.len() as u32),
                    flags: None,
                    composition_time_offset: None,
                });
                audio_data.extend_from_slice(raw_audio);
            }
            let mut traf = TrackFragmentBox::new(1);
            traf.tfhd_box.default_sample_duration = None;
            traf.trun_box.data_offset = Some(0);
            traf.trun_box.samples = audio_samples;
            traf.tfdt_box.base_media_decode_time = audio_units[0].pts as u32;
            segment.moof_box.traf_boxes.push(traf);

            segment.add_track_data((audio_track_id - 1) as usize, &audio_data);

            audio_track.tkhd_box.duration = 0;
            audio_track.mdia_box.mdhd_box.timescale = 1000;
            audio_track.mdia_box.mdhd_box.duration = 0;

            segment.update_offsets();
            segment.write_to(&mut fmp4_data).unwrap();
        }
        AudioType::AAC => {
            let mut sampling_frequency =
                SamplingFrequency::from_frequency(0).unwrap_or_else(|_| SamplingFrequency::Hz48000);
            let mut channel_configuration = ChannelConfiguration::from_u8(0)
                .unwrap_or_else(|_| ChannelConfiguration::TwoChannels);
            let mut profile = AacProfile::Main;

            for a in audio_units.iter() {
                if let Ok(header) = AdtsHeader::read_from(&mut &a.data[..]) {
                    let sample_size: u16 = header.raw_data_blocks_len();
                    sampling_frequency = header.sampling_frequency;
                    channel_configuration = header.channel_configuration;
                    profile = header.profile;
                    let frame_duration = ((1024 as f32 / sampling_frequency.as_u32() as f32)
                        * 1000.0)
                        .round() as u32;

                    audio_samples.push(Sample {
                        duration: Some(frame_duration),
                        size: Some(u32::from(sample_size)),
                        flags: None,
                        composition_time_offset: None,
                    });
                }

                if let Some(frame) = extract_aac_data(&a.data) {
                    audio_data.extend_from_slice(&frame);
                }
            }

            if avc_data.len() > 0 {
                segment.add_track_data(0, &avc_data);
            }

            if !audio_units.is_empty() {
                let mut traf = TrackFragmentBox::new(audio_track_id as u32);
                traf.tfhd_box.default_sample_duration = None;
                traf.trun_box.data_offset = Some(0);
                traf.trun_box.samples = audio_samples;
                traf.tfdt_box.base_media_decode_time =
                    pts_to_ms_timescale(audio_units[0].pts) as u32;
                segment.moof_box.traf_boxes.push(traf);

                segment.add_track_data((audio_track_id - 1) as usize, &audio_data);

                audio_track.tkhd_box.duration = 0;
                audio_track.mdia_box.mdhd_box.timescale = 1000;
                audio_track.mdia_box.mdhd_box.duration = 0;

                let aac_sample_entry = AacSampleEntry {
                    esds_box: Mpeg4EsDescriptorBox {
                        profile,
                        frequency: sampling_frequency,
                        channel_configuration,
                    },
                };
                audio_track
                    .mdia_box
                    .minf_box
                    .stbl_box
                    .stsd_box
                    .sample_entries
                    .push(SampleEntry::Aac(aac_sample_entry));
            }

            segment.update_offsets();
            segment.write_to(&mut fmp4_data).unwrap();
        }
        _ => {}
    }
    // create init.mp4
    let mut segment = InitializationSegment::default();
    segment.moov_box.mvhd_box.timescale = 48000;

    segment.moov_box.mvhd_box.duration = 0;
    segment.moov_box.mvex_box.mehd_box = Some(MovieExtendsHeaderBox {
        fragment_duration: 0,
    });

    if let Some(c) = avcc {
        let mut track = TrackBox::new(1, true);
        let width = 0;
        let height = 0;
        track.tkhd_box.width = (width as u32) << 16;
        track.tkhd_box.height = (height as u32) << 16;
        track.tkhd_box.duration = 0;
        //track.edts_box.elst_box.media_time = start_time;
        track.mdia_box.mdhd_box.timescale = 90000;
        track.mdia_box.mdhd_box.duration = 0;

        let avc_sample_entry = AvcSampleEntry {
            width,
            height,
            avcc_box: AvcConfigurationBox {
                configuration: c.clone(),
            },
        };
        track
            .mdia_box
            .minf_box
            .stbl_box
            .stsd_box
            .sample_entries
            .push(SampleEntry::Avc(avc_sample_entry));
        segment.moov_box.trak_boxes.push(track);
        segment
            .moov_box
            .mvex_box
            .trex_boxes
            .push(TrackExtendsBox::new(1));
    }

    if audio_units.len() > 0 {
        match audio_type {
            AudioType::FLAC => {
                if let Some(frame_info) = frame_info {
                    let mut track = TrackBox::new(audio_track_id, false);
                    let metadata = vec![FLACMetadataBlock {
                        data: create_streaminfo(&frame_info),
                    }];

                    let flac_sample_entry = FLACSampleEntry {
                        dfla_box: mse_fmp4::flac::FLACSpecificBox {
                            metadata_blocks: metadata,
                        },
                        channel_count: frame_info.channels.into(),
                        sample_rate: frame_info.sample_rate.into(),
                        sample_size: (frame_info.bps as u16 * 1000).into(),
                    };
                    track
                        .mdia_box
                        .minf_box
                        .stbl_box
                        .stsd_box
                        .sample_entries
                        .push(SampleEntry::Flac(flac_sample_entry));

                    segment.moov_box.trak_boxes.push(track);

                    segment
                        .moov_box
                        .mvex_box
                        .trex_boxes
                        .push(TrackExtendsBox::new(audio_track_id));
                }
            }
            AudioType::AAC => {
                segment.moov_box.trak_boxes.push(audio_track);
                segment
                    .moov_box
                    .mvex_box
                    .trex_boxes
                    .push(TrackExtendsBox::new(audio_track_id as u32));
            }
            _ => {}
        }
    }

    let _ = segment.write_to(&mut init_data);

    let mut init: Option<Bytes> = None;
    if !init_data.is_empty() {
        init = Some(Bytes::from(init_data))
    }

    Fmp4 {
        init,
        duration: ticks_to_ms(total_ticks) as u32,
        key: is_key,
        data: Bytes::from(fmp4_data),
    }
}
