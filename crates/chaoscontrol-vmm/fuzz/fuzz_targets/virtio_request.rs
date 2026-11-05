#![no_main]
// r[impl chaoscontrol.fuzzing.guest_boundary_targets]

use chaoscontrol_vmm::devices::virtio_request::{
    plan_block_request, plan_chunked_scratch, plan_entropy_request, plan_net_request, NetDirection,
};
use chaoscontrol_vmm::devices::virtio_request::{
    BlockDeviceProfile, BlockRequestHeader, VIRTIO_BLK_T_FLUSH, VIRTIO_BLK_T_IN, VIRTIO_BLK_T_OUT,
};
use chaoscontrol_vmm_fuzz::{parse_input, plan, Reader};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = parse_input(data);
    let Some(chain) = plan(&input) else {
        return;
    };
    let mut reader = Reader::new(data);

    let operation = match reader.u8() % 4 {
        0 => VIRTIO_BLK_T_IN,
        1 => VIRTIO_BLK_T_OUT,
        2 => VIRTIO_BLK_T_FLUSH,
        other => u32::from(other),
    };
    let header = BlockRequestHeader {
        operation,
        reserved: if reader.u8() % 8 == 0 {
            0
        } else {
            reader.u32()
        },
        sector: reader.u64(),
    };
    let device = BlockDeviceProfile {
        disk_size: u64::from(reader.u32()),
        flush_supported: reader.u8() % 2 == 1,
    };
    if let Ok(request) = plan_block_request(&chain, header, device, input.limits) {
        assert!(request.transfer_bytes <= input.limits.max_block_transfer_bytes);
        let end = request
            .disk_offset
            .checked_add(request.transfer_bytes)
            .expect("planned block transfer overflows");
        assert!(
            end <= device.disk_size,
            "planned transfer exceeds the device"
        );
        if header.operation == VIRTIO_BLK_T_FLUSH {
            assert_eq!(request.transfer_bytes, 0);
            assert_eq!(request.disk_offset, 0);
        } else {
            assert_eq!(
                request.disk_offset,
                header.sector.checked_mul(512).expect("sector scaling")
            );
        }
        assert_eq!(
            Some(request.status_address),
            chain.buffers().last().map(|buffer| buffer.addr)
        );
    }

    let direction = if reader.u8() % 2 == 0 {
        NetDirection::Receive
    } else {
        NetDirection::Transmit
    };
    let packet_bytes = u64::from(reader.u16()) % (input.limits.max_net_frame_bytes + 2);
    if let Ok(request) = plan_net_request(&chain, direction, packet_bytes, input.limits) {
        assert!(request.packet_bytes <= input.limits.max_net_frame_bytes);
        assert_eq!(request.direction, direction);
    }

    if let Ok(request) = plan_entropy_request(&chain, input.limits) {
        assert_eq!(request.transfer_bytes, chain.aggregate_length());
        assert!(request.transfer_bytes <= input.limits.max_entropy_transfer_bytes);
    }

    let scratch_request = u64::from(reader.u32()) * u64::from(reader.u8());
    let scratch_maximum = usize::from(reader.u16());
    if let Ok((requested, maximum)) = plan_chunked_scratch(scratch_request, scratch_maximum) {
        assert!(requested <= maximum);
    }
});
