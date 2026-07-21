use bytes::Bytes;

#[derive(Clone, Debug)]
pub struct AvcDecoderConfigurationRecord {
    pub profile_idc: u8,
    pub constraint_set_flag: u8,
    pub level_idc: u8,
    pub sequence_parameter_set: Bytes,
    pub picture_parameter_set: Bytes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AacProfile {
    Main = 0,
    Lc = 1,
    Ssr = 2,
    Ltp = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SamplingFrequency {
    Hz96000 = 0,
    Hz88200 = 1,
    Hz64000 = 2,
    Hz48000 = 3,
    Hz44100 = 4,
    Hz32000 = 5,
    Hz24000 = 6,
    Hz22050 = 7,
    Hz16000 = 8,
    Hz12000 = 9,
    Hz11025 = 10,
    Hz8000 = 11,
    Hz7350 = 12,
}

impl SamplingFrequency {
    pub fn as_u32(self) -> u32 {
        match self {
            SamplingFrequency::Hz96000 => 96_000,
            SamplingFrequency::Hz88200 => 88_200,
            SamplingFrequency::Hz64000 => 64_000,
            SamplingFrequency::Hz48000 => 48_000,
            SamplingFrequency::Hz44100 => 44_100,
            SamplingFrequency::Hz32000 => 32_000,
            SamplingFrequency::Hz24000 => 24_000,
            SamplingFrequency::Hz22050 => 22_050,
            SamplingFrequency::Hz16000 => 16_000,
            SamplingFrequency::Hz12000 => 12_000,
            SamplingFrequency::Hz11025 => 11_025,
            SamplingFrequency::Hz8000 => 8_000,
            SamplingFrequency::Hz7350 => 7_350,
        }
    }

    fn as_index(self) -> u8 {
        self as u8
    }

    fn from_index(index: u8) -> Option<Self> {
        Some(match index {
            0 => SamplingFrequency::Hz96000,
            1 => SamplingFrequency::Hz88200,
            2 => SamplingFrequency::Hz64000,
            3 => SamplingFrequency::Hz48000,
            4 => SamplingFrequency::Hz44100,
            5 => SamplingFrequency::Hz32000,
            6 => SamplingFrequency::Hz24000,
            7 => SamplingFrequency::Hz22050,
            8 => SamplingFrequency::Hz16000,
            9 => SamplingFrequency::Hz12000,
            10 => SamplingFrequency::Hz11025,
            11 => SamplingFrequency::Hz8000,
            12 => SamplingFrequency::Hz7350,
            _ => return None,
        })
    }

    pub fn from_frequency(frequency: u32) -> Option<Self> {
        Some(match frequency {
            96_000 => SamplingFrequency::Hz96000,
            88_200 => SamplingFrequency::Hz88200,
            64_000 => SamplingFrequency::Hz64000,
            48_000 => SamplingFrequency::Hz48000,
            44_100 => SamplingFrequency::Hz44100,
            32_000 => SamplingFrequency::Hz32000,
            24_000 => SamplingFrequency::Hz24000,
            22_050 => SamplingFrequency::Hz22050,
            16_000 => SamplingFrequency::Hz16000,
            12_000 => SamplingFrequency::Hz12000,
            11_025 => SamplingFrequency::Hz11025,
            8_000 => SamplingFrequency::Hz8000,
            7_350 => SamplingFrequency::Hz7350,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelConfiguration {
    SentViaInbandPce = 0,
    OneChannel = 1,
    TwoChannels = 2,
    ThreeChannels = 3,
    FourChannels = 4,
    FiveChannels = 5,
    SixChannels = 6,
    EightChannels = 7,
}

impl ChannelConfiguration {
    fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => ChannelConfiguration::SentViaInbandPce,
            1 => ChannelConfiguration::OneChannel,
            2 => ChannelConfiguration::TwoChannels,
            3 => ChannelConfiguration::ThreeChannels,
            4 => ChannelConfiguration::FourChannels,
            5 => ChannelConfiguration::FiveChannels,
            6 => ChannelConfiguration::SixChannels,
            7 => ChannelConfiguration::EightChannels,
            _ => return None,
        })
    }

    fn channels(self) -> u16 {
        match self {
            ChannelConfiguration::SentViaInbandPce => 2,
            ChannelConfiguration::OneChannel => 1,
            ChannelConfiguration::TwoChannels => 2,
            ChannelConfiguration::ThreeChannels => 3,
            ChannelConfiguration::FourChannels => 4,
            ChannelConfiguration::FiveChannels => 5,
            ChannelConfiguration::SixChannels => 6,
            ChannelConfiguration::EightChannels => 8,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AdtsHeader {
    pub profile: AacProfile,
    pub sampling_frequency: SamplingFrequency,
    pub channel_configuration: ChannelConfiguration,
}

impl AdtsHeader {
    pub fn read_from(data: &[u8]) -> Option<Self> {
        if data.len() < 7 || data[0] != 0xff || (data[1] & 0xf0) != 0xf0 {
            return None;
        }

        let mpeg_version = (data[1] >> 3) & 0x01;
        let layer = (data[1] >> 1) & 0x03;
        if mpeg_version != 0 || layer != 0 {
            return None;
        }

        let profile = match data[2] >> 6 {
            0 => AacProfile::Main,
            1 => AacProfile::Lc,
            2 => AacProfile::Ssr,
            3 => AacProfile::Ltp,
            _ => return None,
        };
        let sampling_frequency = SamplingFrequency::from_index((data[2] >> 2) & 0x0f)?;
        let channel_bits = ((data[2] & 0x01) << 2) | (data[3] >> 6);
        let channel_configuration = ChannelConfiguration::from_u8(channel_bits)?;

        Some(Self {
            profile,
            sampling_frequency,
            channel_configuration,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SampleFlags {
    pub is_leading: u8,
    pub sample_depends_on: u8,
    pub sample_is_depended_on: u8,
    pub sample_has_redundancy: u8,
    pub sample_padding_value: u8,
    pub sample_is_non_sync_sample: bool,
    pub sample_degradation_priority: u16,
}

impl SampleFlags {
    fn as_u32(self) -> u32 {
        (u32::from(self.is_leading) << 26)
            | (u32::from(self.sample_depends_on) << 24)
            | (u32::from(self.sample_is_depended_on) << 22)
            | (u32::from(self.sample_has_redundancy) << 20)
            | (u32::from(self.sample_padding_value) << 17)
            | ((self.sample_is_non_sync_sample as u32) << 16)
            | u32::from(self.sample_degradation_priority)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FragmentSample {
    pub duration: Option<u32>,
    pub size: Option<u32>,
    pub flags: Option<SampleFlags>,
    pub composition_time_offset: Option<i32>,
}

impl FragmentSample {
    fn trun_flags(self) -> u32 {
        (self.duration.is_some() as u32 * 0x00_0100)
            | (self.size.is_some() as u32 * 0x00_0200)
            | (self.flags.is_some() as u32 * 0x00_0400)
            | (self.composition_time_offset.is_some() as u32 * 0x00_0800)
    }
}

#[derive(Debug)]
pub struct FragmentTrack<'a> {
    pub track_id: u32,
    pub base_media_decode_time: u64,
    pub samples: Vec<FragmentSample>,
    pub data: &'a [u8],
}

#[derive(Clone, Debug)]
pub struct VideoInit {
    pub track_id: u32,
    pub width: u16,
    pub height: u16,
    pub avcc: AvcDecoderConfigurationRecord,
}

#[derive(Clone, Debug)]
pub enum AudioInit {
    Aac {
        track_id: u32,
        profile: AacProfile,
        frequency: SamplingFrequency,
        channel_configuration: ChannelConfiguration,
    },
    Flac {
        track_id: u32,
        channel_count: u16,
        sample_size: u16,
        sample_rate: u32,
        streaminfo: Vec<u8>,
    },
    Pcm {
        track_id: u32,
        channel_count: u16,
        sample_size: u8,
        sample_rate: u32,
        little_endian: bool,
        floating_point: bool,
    },
    Opus {
        track_id: u32,
        input_sample_rate: u32,
        channel_count: u16,
        pre_skip: u16,
        output_gain: i16,
    },
}

pub fn write_media_segment(
    out: &mut Vec<u8>,
    sequence_number: u32,
    tracks: &[FragmentTrack<'_>],
) -> Option<()> {
    let start = out.len();
    let result = write_media_segment_inner(out, sequence_number, tracks);
    if result.is_none() {
        out.truncate(start);
    }
    result
}

fn write_media_segment_inner(
    out: &mut Vec<u8>,
    sequence_number: u32,
    tracks: &[FragmentTrack<'_>],
) -> Option<()> {
    if tracks.is_empty() {
        return None;
    }

    let moof_start = out.len();
    let mut trun_data_offset_positions = Vec::with_capacity(tracks.len());
    write_moof(
        out,
        sequence_number,
        tracks,
        &mut trun_data_offset_positions,
    )?;

    let moof_size = u64::try_from(out.len().checked_sub(moof_start)?).ok()?;
    let mut track_data_offset = 0u64;
    for (position, track) in trun_data_offset_positions.iter().copied().zip(tracks) {
        let data_offset = moof_size.checked_add(8)?.checked_add(track_data_offset)?;
        patch_i32(out, position, i32::try_from(data_offset).ok()?);
        track_data_offset = track_data_offset.checked_add(u64::try_from(track.data.len()).ok()?)?;
    }

    write_box(out, *b"mdat", |out| {
        for track in tracks {
            out.extend_from_slice(track.data);
        }
        Some(())
    })?;

    Some(())
}

pub fn write_init_segment(
    out: &mut Vec<u8>,
    movie_timescale: u32,
    video: Option<&VideoInit>,
    audio: Option<&AudioInit>,
) -> Option<()> {
    let start = out.len();
    let result = write_init_segment_inner(out, movie_timescale, video, audio);
    if result.is_none() {
        out.truncate(start);
    }
    result
}

fn write_init_segment_inner(
    out: &mut Vec<u8>,
    movie_timescale: u32,
    video: Option<&VideoInit>,
    audio: Option<&AudioInit>,
) -> Option<()> {
    if video.is_none() && audio.is_none() {
        return None;
    }

    write_ftyp(out)?;
    write_box(out, *b"moov", |out| {
        write_mvhd(out, movie_timescale, 0)?;
        if let Some(video) = video {
            write_trak(out, video.track_id, true, 90_000, 0, Some(video), None)?;
        }
        if let Some(audio) = audio {
            write_trak(
                out,
                audio.track_id(),
                false,
                audio.timescale(),
                0,
                None,
                Some(audio),
            )?;
        }
        write_mvex(
            out,
            video.map(|v| v.track_id),
            audio.map(AudioInit::track_id),
        )?;
        Some(())
    })
}

impl AudioInit {
    fn track_id(&self) -> u32 {
        match self {
            AudioInit::Aac { track_id, .. }
            | AudioInit::Flac { track_id, .. }
            | AudioInit::Pcm { track_id, .. }
            | AudioInit::Opus { track_id, .. } => *track_id,
        }
    }

    pub(crate) fn timescale(&self) -> u32 {
        match self {
            AudioInit::Opus { .. } => 48_000,
            AudioInit::Aac { frequency, .. } => frequency.as_u32(),
            AudioInit::Flac { .. } | AudioInit::Pcm { .. } => 1_000,
        }
    }
}

fn write_moof(
    out: &mut Vec<u8>,
    sequence_number: u32,
    tracks: &[FragmentTrack<'_>],
    trun_data_offset_positions: &mut Vec<usize>,
) -> Option<()> {
    write_box(out, *b"moof", |out| {
        write_full_box(out, *b"mfhd", 0, 0, |out| {
            write_u32(out, sequence_number);
            Some(())
        })?;
        for track in tracks {
            write_traf(out, track, trun_data_offset_positions)?;
        }
        Some(())
    })
}

fn write_traf(
    out: &mut Vec<u8>,
    track: &FragmentTrack<'_>,
    trun_data_offset_positions: &mut Vec<usize>,
) -> Option<()> {
    write_box(out, *b"traf", |out| {
        write_full_box(out, *b"tfhd", 0, 0x02_0000, |out| {
            write_u32(out, track.track_id);
            Some(())
        })?;
        write_tfdt(out, track.base_media_decode_time)?;
        write_trun(out, &track.samples, trun_data_offset_positions)?;
        Some(())
    })
}

fn write_tfdt(out: &mut Vec<u8>, base_media_decode_time: u64) -> Option<()> {
    let version = if base_media_decode_time > u64::from(u32::MAX) {
        1
    } else {
        0
    };
    write_full_box(out, *b"tfdt", version, 0, |out| {
        if version == 1 {
            write_u64(out, base_media_decode_time);
        } else {
            write_u32(out, base_media_decode_time as u32);
        }
        Some(())
    })
}

fn write_trun(
    out: &mut Vec<u8>,
    samples: &[FragmentSample],
    trun_data_offset_positions: &mut Vec<usize>,
) -> Option<()> {
    let sample_flags = samples.first().copied().unwrap_or_default().trun_flags();
    write_full_box(out, *b"trun", 1, 0x00_0001 | sample_flags, |out| {
        write_u32(out, u32::try_from(samples.len()).ok()?);
        trun_data_offset_positions.push(out.len());
        write_i32(out, 0);
        for sample in samples {
            if let Some(duration) = sample.duration {
                write_u32(out, duration);
            }
            if let Some(size) = sample.size {
                write_u32(out, size);
            }
            if let Some(flags) = sample.flags {
                write_u32(out, flags.as_u32());
            }
            if let Some(offset) = sample.composition_time_offset {
                write_i32(out, offset);
            }
        }
        Some(())
    })
}

fn write_ftyp(out: &mut Vec<u8>) -> Option<()> {
    write_box(out, *b"ftyp", |out| {
        out.extend_from_slice(b"mp42");
        write_u32(out, 1);
        out.extend_from_slice(b"mp41");
        out.extend_from_slice(b"mp42");
        out.extend_from_slice(b"isom");
        out.extend_from_slice(b"hlsf");
        Some(())
    })
}

fn write_mvhd(out: &mut Vec<u8>, timescale: u32, duration: u32) -> Option<()> {
    write_full_box(out, *b"mvhd", 0, 0, |out| {
        write_u32(out, 0);
        write_u32(out, 0);
        write_u32(out, timescale);
        write_u32(out, duration);
        write_i32(out, 0x0001_0000);
        write_i16(out, 256);
        write_zeroes(out, 2);
        write_zeroes(out, 8);
        write_matrix(out);
        write_zeroes(out, 24);
        write_u32(out, u32::MAX);
        Some(())
    })
}

fn write_trak(
    out: &mut Vec<u8>,
    track_id: u32,
    is_video: bool,
    timescale: u32,
    duration: u32,
    video: Option<&VideoInit>,
    audio: Option<&AudioInit>,
) -> Option<()> {
    write_box(out, *b"trak", |out| {
        let (width, height) = video
            .map(|video| (u32::from(video.width) << 16, u32::from(video.height) << 16))
            .unwrap_or((0, 0));
        write_tkhd(out, track_id, duration, width, height)?;
        write_mdia(out, is_video, timescale, duration, video, audio)?;
        Some(())
    })
}

fn write_tkhd(
    out: &mut Vec<u8>,
    track_id: u32,
    duration: u32,
    width: u32,
    height: u32,
) -> Option<()> {
    write_full_box(out, *b"tkhd", 0, 0x00_0007, |out| {
        write_u32(out, 0);
        write_u32(out, 0);
        write_u32(out, track_id);
        write_zeroes(out, 4);
        write_u32(out, duration);
        write_zeroes(out, 8);
        write_i16(out, 0);
        write_i16(out, 0);
        write_i16(out, 256);
        write_zeroes(out, 2);
        write_matrix(out);
        write_u32(out, width);
        write_u32(out, height);
        Some(())
    })
}

fn write_mdia(
    out: &mut Vec<u8>,
    is_video: bool,
    timescale: u32,
    duration: u32,
    video: Option<&VideoInit>,
    audio: Option<&AudioInit>,
) -> Option<()> {
    write_box(out, *b"mdia", |out| {
        write_mdhd(out, timescale, duration)?;
        write_hdlr(out, is_video)?;
        write_minf(out, is_video, video, audio)?;
        Some(())
    })
}

fn write_mdhd(out: &mut Vec<u8>, timescale: u32, duration: u32) -> Option<()> {
    write_full_box(out, *b"mdhd", 0, 0, |out| {
        write_u32(out, 0);
        write_u32(out, 0);
        write_u32(out, timescale);
        write_u32(out, duration);
        write_u16(out, 0x55c4);
        write_zeroes(out, 2);
        Some(())
    })
}

fn write_hdlr(out: &mut Vec<u8>, is_video: bool) -> Option<()> {
    let (handler_type, name): ([u8; 4], &[u8]) = if is_video {
        (*b"vide", b"Video Handler\0")
    } else {
        (*b"soun", b"Sound Handler\0")
    };

    write_full_box(out, *b"hdlr", 0, 0, |out| {
        write_zeroes(out, 4);
        out.extend_from_slice(&handler_type);
        write_zeroes(out, 12);
        out.extend_from_slice(name);
        Some(())
    })
}

fn write_minf(
    out: &mut Vec<u8>,
    is_video: bool,
    video: Option<&VideoInit>,
    audio: Option<&AudioInit>,
) -> Option<()> {
    write_box(out, *b"minf", |out| {
        if is_video {
            write_vmhd(out)?;
        } else {
            write_smhd(out)?;
        }
        write_dinf(out)?;
        write_stbl(out, video, audio)?;
        Some(())
    })
}

fn write_vmhd(out: &mut Vec<u8>) -> Option<()> {
    write_full_box(out, *b"vmhd", 0, 1, |out| {
        write_u16(out, 0);
        write_zeroes(out, 6);
        Some(())
    })
}

fn write_smhd(out: &mut Vec<u8>) -> Option<()> {
    write_full_box(out, *b"smhd", 0, 0, |out| {
        write_i16(out, 0);
        write_zeroes(out, 2);
        Some(())
    })
}

fn write_dinf(out: &mut Vec<u8>) -> Option<()> {
    write_box(out, *b"dinf", |out| {
        write_full_box(out, *b"dref", 0, 0, |out| {
            write_u32(out, 1);
            write_full_box(out, *b"url ", 0, 1, |_| Some(()))?;
            Some(())
        })
    })
}

fn write_stbl(
    out: &mut Vec<u8>,
    video: Option<&VideoInit>,
    audio: Option<&AudioInit>,
) -> Option<()> {
    write_box(out, *b"stbl", |out| {
        write_stsd(out, video, audio)?;
        write_full_box(out, *b"stts", 0, 0, |out| {
            write_u32(out, 0);
            Some(())
        })?;
        write_full_box(out, *b"stsc", 0, 0, |out| {
            write_u32(out, 0);
            Some(())
        })?;
        write_full_box(out, *b"stsz", 0, 0, |out| {
            write_u32(out, 0);
            write_u32(out, 0);
            Some(())
        })?;
        write_full_box(out, *b"stco", 0, 0, |out| {
            write_u32(out, 0);
            Some(())
        })?;
        Some(())
    })
}

fn write_stsd(
    out: &mut Vec<u8>,
    video: Option<&VideoInit>,
    audio: Option<&AudioInit>,
) -> Option<()> {
    write_full_box(out, *b"stsd", 0, 0, |out| {
        write_u32(out, 1);
        if let Some(video) = video {
            write_avc1(out, video)?;
        } else if let Some(audio) = audio {
            match audio {
                AudioInit::Aac {
                    profile,
                    frequency,
                    channel_configuration,
                    ..
                } => write_mp4a(out, *profile, *frequency, *channel_configuration)?,
                AudioInit::Flac {
                    channel_count,
                    sample_size,
                    sample_rate,
                    streaminfo,
                    ..
                } => write_flac(out, *channel_count, *sample_size, *sample_rate, streaminfo)?,
                AudioInit::Pcm {
                    channel_count,
                    sample_size,
                    sample_rate,
                    little_endian,
                    floating_point,
                    ..
                } => write_pcm(
                    out,
                    *channel_count,
                    *sample_size,
                    *sample_rate,
                    *little_endian,
                    *floating_point,
                )?,
                AudioInit::Opus {
                    input_sample_rate,
                    channel_count,
                    pre_skip,
                    output_gain,
                    ..
                } => write_opus(
                    out,
                    *input_sample_rate,
                    *channel_count,
                    *pre_skip,
                    *output_gain,
                )?,
            }
        } else {
            return None;
        }
        Some(())
    })
}

fn write_avc1(out: &mut Vec<u8>, video: &VideoInit) -> Option<()> {
    write_box(out, *b"avc1", |out| {
        write_zeroes(out, 6);
        write_u16(out, 1);
        write_zeroes(out, 16);
        write_u16(out, video.width);
        write_u16(out, video.height);
        write_u32(out, 0x0048_0000);
        write_u32(out, 0x0048_0000);
        write_zeroes(out, 4);
        write_u16(out, 1);
        write_zeroes(out, 32);
        write_u16(out, 0x0018);
        write_i16(out, -1);
        write_avcc(out, &video.avcc)?;
        Some(())
    })
}

fn write_avcc(out: &mut Vec<u8>, avcc: &AvcDecoderConfigurationRecord) -> Option<()> {
    write_box(out, *b"avcC", |out| {
        write_u8(out, 1);
        write_u8(out, avcc.profile_idc);
        write_u8(out, avcc.constraint_set_flag);
        write_u8(out, avcc.level_idc);
        write_u8(out, 0b1111_1100 | 0b0000_0011);
        write_u8(out, 0b1110_0000 | 0b0000_0001);
        write_u16(out, u16::try_from(avcc.sequence_parameter_set.len()).ok()?);
        out.extend_from_slice(&avcc.sequence_parameter_set);
        write_u8(out, 1);
        write_u16(out, u16::try_from(avcc.picture_parameter_set.len()).ok()?);
        out.extend_from_slice(&avcc.picture_parameter_set);
        Some(())
    })
}

fn write_mp4a(
    out: &mut Vec<u8>,
    profile: AacProfile,
    frequency: SamplingFrequency,
    channel_configuration: ChannelConfiguration,
) -> Option<()> {
    write_box(out, *b"mp4a", |out| {
        write_zeroes(out, 6);
        write_u16(out, 1);
        write_zeroes(out, 8);
        write_u16(out, channel_configuration.channels());
        write_u16(out, 16);
        write_zeroes(out, 4);
        write_u16(out, u16::try_from(frequency.as_u32()).ok()?);
        write_zeroes(out, 2);
        write_esds(out, profile, frequency, channel_configuration)?;
        Some(())
    })
}

fn write_esds(
    out: &mut Vec<u8>,
    profile: AacProfile,
    frequency: SamplingFrequency,
    channel_configuration: ChannelConfiguration,
) -> Option<()> {
    write_full_box(out, *b"esds", 0, 0, |out| {
        write_u8(out, 0x03);
        write_u8(out, 25);
        write_u16(out, 0);
        write_u8(out, 0);

        write_u8(out, 0x04);
        write_u8(out, 17);
        write_u8(out, 0x40);
        write_u8(out, (5 << 2) | 1);
        write_u24(out, 0);
        write_u32(out, 0);
        write_u32(out, 0);

        write_u8(out, 0x05);
        write_u8(out, 2);
        write_u16(
            out,
            (((profile as u16) + 1) << 11)
                | (u16::from(frequency.as_index()) << 7)
                | ((channel_configuration as u16) << 3),
        );

        write_u8(out, 0x06);
        write_u8(out, 1);
        write_u8(out, 2);
        Some(())
    })
}

fn write_flac(
    out: &mut Vec<u8>,
    channel_count: u16,
    sample_size: u16,
    sample_rate: u32,
    streaminfo: &[u8],
) -> Option<()> {
    write_box(out, *b"fLaC", |out| {
        write_zeroes(out, 6);
        write_u16(out, 1);
        write_zeroes(out, 8);
        write_u16(out, channel_count);
        write_u16(out, sample_size);
        write_zeroes(out, 4);
        write_u32(out, sample_rate.checked_shl(16)?);
        write_dfla(out, streaminfo)?;
        Some(())
    })
}

fn write_dfla(out: &mut Vec<u8>, streaminfo: &[u8]) -> Option<()> {
    write_box(out, *b"dfLa", |out| {
        write_u32(out, 0);
        let block_len = u32::try_from(streaminfo.len()).ok()?;
        if block_len > 0x00ff_ffff {
            return None;
        }
        write_u32(out, (1 << 31) | block_len);
        out.extend_from_slice(streaminfo);
        Some(())
    })
}

fn write_pcm(
    out: &mut Vec<u8>,
    channel_count: u16,
    sample_size: u8,
    sample_rate: u32,
    little_endian: bool,
    floating_point: bool,
) -> Option<()> {
    let sample_entry = if floating_point { *b"fpcm" } else { *b"ipcm" };
    write_box(out, sample_entry, |out| {
        write_zeroes(out, 6);
        write_u16(out, 1);
        write_zeroes(out, 8);
        write_u16(out, channel_count);
        write_u16(out, u16::from(sample_size));
        write_zeroes(out, 4);
        write_u32(out, sample_rate.checked_shl(16)?);
        write_pcmc(out, sample_size, little_endian)?;
        write_chnl_unknown_positions(out, channel_count)?;
        Some(())
    })
}

fn write_opus(
    out: &mut Vec<u8>,
    input_sample_rate: u32,
    channel_count: u16,
    pre_skip: u16,
    output_gain: i16,
) -> Option<()> {
    if input_sample_rate == 0 || !matches!(channel_count, 1 | 2) {
        return None;
    }
    write_box(out, *b"Opus", |out| {
        write_zeroes(out, 6);
        write_u16(out, 1);
        write_zeroes(out, 8);
        write_u16(out, channel_count);
        write_u16(out, 16);
        write_zeroes(out, 4);
        write_u32(out, 48_000_u32.checked_shl(16)?);
        write_dops(
            out,
            input_sample_rate,
            u8::try_from(channel_count).ok()?,
            pre_skip,
            output_gain,
        )?;
        Some(())
    })
}

fn write_dops(
    out: &mut Vec<u8>,
    input_sample_rate: u32,
    channel_count: u8,
    pre_skip: u16,
    output_gain: i16,
) -> Option<()> {
    write_box(out, *b"dOps", |out| {
        write_u8(out, 0);
        write_u8(out, channel_count);
        write_u16(out, pre_skip);
        write_u32(out, input_sample_rate);
        write_i16(out, output_gain);
        // Channel mapping family zero is the standardized mono/stereo mapping.
        write_u8(out, 0);
        Some(())
    })
}

fn write_pcmc(out: &mut Vec<u8>, sample_size: u8, little_endian: bool) -> Option<()> {
    write_full_box(out, *b"pcmC", 0, 0, |out| {
        write_u8(out, u8::from(little_endian));
        write_u8(out, sample_size);
        Some(())
    })
}

fn write_chnl_unknown_positions(out: &mut Vec<u8>, channel_count: u16) -> Option<()> {
    write_full_box(out, *b"chnl", 0, 0, |out| {
        // Channel-structured, custom layout. DAW stems do not necessarily map
        // to loudspeakers, so preserve exact source order and mark each
        // position as the standardized unknown/undefined value.
        write_u8(out, 1);
        write_u8(out, 0);
        for _ in 0..channel_count {
            write_u8(out, 127);
        }
        Some(())
    })
}

fn write_mvex(
    out: &mut Vec<u8>,
    video_track_id: Option<u32>,
    audio_track_id: Option<u32>,
) -> Option<()> {
    write_box(out, *b"mvex", |out| {
        write_full_box(out, *b"mehd", 0, 0, |out| {
            write_u32(out, 0);
            Some(())
        })?;
        if let Some(track_id) = video_track_id {
            write_trex(out, track_id)?;
        }
        if let Some(track_id) = audio_track_id {
            write_trex(out, track_id)?;
        }
        Some(())
    })
}

fn write_trex(out: &mut Vec<u8>, track_id: u32) -> Option<()> {
    write_full_box(out, *b"trex", 0, 0, |out| {
        write_u32(out, track_id);
        write_u32(out, 1);
        write_u32(out, 0);
        write_u32(out, 0);
        write_u32(out, 0);
        Some(())
    })
}

fn write_full_box<F>(out: &mut Vec<u8>, name: [u8; 4], version: u8, flags: u32, f: F) -> Option<()>
where
    F: FnOnce(&mut Vec<u8>) -> Option<()>,
{
    write_box(out, name, |out| {
        write_u32(out, (u32::from(version) << 24) | (flags & 0x00ff_ffff));
        f(out)
    })
}

fn write_box<F>(out: &mut Vec<u8>, name: [u8; 4], f: F) -> Option<()>
where
    F: FnOnce(&mut Vec<u8>) -> Option<()>,
{
    let start = out.len();
    write_u32(out, 0);
    out.extend_from_slice(&name);
    f(out)?;
    let size = u32::try_from(out.len().checked_sub(start)?).ok()?;
    out[start..start + 4].copy_from_slice(&size.to_be_bytes());
    Some(())
}

fn patch_i32(out: &mut [u8], position: usize, value: i32) {
    out[position..position + 4].copy_from_slice(&value.to_be_bytes());
}

fn write_matrix(out: &mut Vec<u8>) {
    for value in [0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
        write_i32(out, value);
    }
}

fn write_zeroes(out: &mut Vec<u8>, count: usize) {
    out.resize(out.len() + count, 0);
}

fn write_u8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn write_i16(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn write_u24(out: &mut Vec<u8>, value: u32) {
    out.push(((value >> 16) & 0xff) as u8);
    out.push(((value >> 8) & 0xff) as u8);
    out.push((value & 0xff) as u8);
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn write_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn write_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}
