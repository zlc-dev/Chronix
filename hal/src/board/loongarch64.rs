pub const MEMORY_END: usize = 0x9600_0000;

pub const MMIO: &[(usize, usize)] = &[
    (0x0010_0000, 0x00_2000), // VIRT_TEST/RTC  in virt machine
    (0x1000_1000, 0x00_1000), // Virtio Block in virt machine
    // (0x1fe0_01e0, 0x00_0100), // UART in virt machine
];

pub const MAX_PROCESSORS: usize = 4;