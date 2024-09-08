use mse_fmp4::flac::{FlacMetadataBlock, FlacStreamConfiguration};

pub(crate) fn parse_flac_header(data: &[u8]) -> (FlacStreamConfiguration, Vec<FlacMetadataBlock>) {
    let mut config = FlacStreamConfiguration {
        min_block_size: 0,
        max_block_size: 0,
        min_frame_size: 0,
        max_frame_size: 0,
        sample_rate: 0,
        channels: 0,
        bits_per_sample: 0,
    };
    let mut metadata = Vec::new();

    let mut offset = 0;

    // Parse STREAMINFO block
    if data[offset..offset + 4] == [0x66, 0x4C, 0x61, 0x43] {
        // "fLaC"
        offset += 4;
    }

    let is_last = (data[offset] & 0x80) != 0;
    let block_type = data[offset] & 0x7F;
    let length = u32::from_be_bytes([
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
        data[offset + 4],
    ]);
    offset += 5;

    if block_type == 0 {
        // STREAMINFO
        config.min_block_size = u16::from_be_bytes([data[offset], data[offset + 1]]);
        config.max_block_size = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
        config.min_frame_size =
            u32::from_be_bytes([data[offset + 4], data[offset + 5], data[offset + 6], 0]);
        config.max_frame_size =
            u32::from_be_bytes([data[offset + 7], data[offset + 8], data[offset + 9], 0]);
        config.sample_rate =
            u32::from_be_bytes([data[offset + 10], data[offset + 11], data[offset + 12], 0]) >> 4;
        config.channels = ((data[offset + 12] & 0x0E) >> 1) + 1;
        config.bits_per_sample =
            ((data[offset + 12] & 0x01) << 4) | ((data[offset + 13] & 0xF0) >> 4) + 1;
        offset += length as usize;
    }

    // Parse other metadata blocks
    while !is_last && offset < data.len() {
        let is_last = (data[offset] & 0x80) != 0;
        let block_type = data[offset] & 0x7F;
        let length = u32::from_be_bytes([
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
            data[offset + 4],
        ]);
        offset += 5;

        let block_data = data[offset..offset + length as usize].to_vec();
        metadata.push(FlacMetadataBlock {
            last_metadata_block_flag: is_last,
            block_type,
            length,
            data: block_data,
        });

        offset += length as usize;
        if is_last {
            break;
        }
    }

    (config, metadata)
}
