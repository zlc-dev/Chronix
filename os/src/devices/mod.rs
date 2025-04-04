#![allow(dead_code)]
pub mod net;
use core::any::Any;
use alloc::{boxed::Box, string::String, sync::Arc};
use async_trait::async_trait;
use downcast_rs::DowncastSync;
use net::{EthernetAddress, NetBufPtr};
use smoltcp::phy::{DeviceCapabilities,RxToken, TxToken};


/// General Device Operations
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DeviceType {
    Block,
    Char,
    Net,
    Display,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord)]
#[repr(usize)]
pub enum DeviceMajor {
    Serial = 4,
    Block = 8,
    Net = 9,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DevId {
    /// Major Device Number
    pub major: DeviceMajor,
    /// Minor Device Number. It Identifies different device instances of the
    /// same type
    pub minor: usize,
}

/// meta data for any devices
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceMeta {
    /// Device id.
    pub dev_id: DevId,
    /// Name of the device.
    pub name: String,
    /// Mmio start address.
    pub mmio_base: usize,
    /// Mmio size.
    pub mmio_size: usize,
    /// Interrupt number.
    pub irq_no: Option<usize>,
    /// Device type.
    pub dtype: DeviceType,
}

/// The error type for device operation failures.
#[derive(Debug)]
pub enum DevError {
    /// An entity already exists.
    AlreadyExists,
    /// Try again, for non-blocking APIs.
    Again,
    /// Bad internal state.
    BadState,
    /// Invalid parameter/argument.
    InvalidParam,
    /// Input/output error.
    Io,
    /// Not enough space/cannot allocate memory (DMA).
    NoMemory,
    /// Device or resource is busy.
    ResourceBusy,
    /// This operation is unsupported or unimplemented.
    Unsupported,
}

/// A specialized `Result` type for device operations.
pub type DevResult<T = ()> = Result<T, DevError>;

pub trait Device: Sync + Send + DowncastSync {
    fn meta(&self) -> &DeviceMeta;

    fn init(&self);

    fn handle_irq(&self);

    fn dev_id(&self) -> DevId {
        self.meta().dev_id
    }

    fn name(&self) -> &str {
        &self.meta().name
    }

    fn mmio_base(&self) -> usize {
        self.meta().mmio_base
    }

    fn mmio_size(&self) -> usize {
        self.meta().mmio_size
    }

    fn irq_no(&self) -> Option<usize> {
        self.meta().irq_no
    }

    fn dtype(&self) -> DeviceType {
        self.meta().dtype
    }

    fn as_blk(self: Arc<Self>) -> Option<Arc<dyn BlockDevice>> {
        None
    }

    fn as_char(self: Arc<Self>) -> Option<Arc<dyn CharDevice>> {
        None
    }

    fn as_net(self: Arc<Self>) -> Option<Arc<dyn NetDevice>> {
        None
    }
}

/// Trait for block devices
/// which reads and writes data in the unit of blocks
pub trait BlockDevice: Send + Sync + Any {
    fn size(&self) -> u64;

    fn block_size(&self) -> usize;

    /// Read data form block to buffer
    fn read_block(&self, block_id: usize, buf: &mut [u8]);

    /// Write data from buffer to block
    fn write_block(&self, block_id: usize, buf: &[u8]);
}

pub trait NetDevice: Send + Sync + Any {
    // ! smoltcp demands that the device must have below trait
    ///Get a description of device capabilities.
    fn capabilities(&self) -> DeviceCapabilities;
    /// Construct a token pair consisting of one receive token and one transmit token.
    fn receive(&mut self) ->  DevResult<Box<dyn NetBufPtrTrait>>;
    /// Transmits a packet in the buffer to the network, without blocking,
    fn transmit(&mut self, tx_buf: Box<dyn NetBufPtrTrait>) -> DevResult; 
    // ! method in implementing a network device concering buffer management
    /// allocate a tx buffer
    fn alloc_tx_buffer(&mut self, size: usize) -> DevResult<Box<dyn NetBufPtrTrait>>;
    /// recycle buf when rx complete
    fn recycle_rx_buffer(&mut self, rx_buf: Box<dyn NetBufPtrTrait>) -> DevResult;
    /// recycle used tx buffer
    fn recycle_tx_buffer(&mut self) -> DevResult;
    #[allow(dead_code)]
    /// ethernet address of the NIC
    fn mac_address(&self) -> EthernetAddress;
}

pub trait NetBufPtrTrait: Any {
    fn packet(&self) -> &[u8];
    fn packet_mut(&mut self) -> &mut [u8];
    fn packet_len(&self) -> usize;
}


#[async_trait]
pub trait CharDevice: Send + Sync + Any {
    /// read data to given buffer
    async fn read(&self, buf: &mut [u8]) -> usize;
    /// write data using given buffer
    async fn write(&self, buf: &[u8]) -> usize;
    /// if there is data waiting to be read
    async fn poll_in(&self) -> bool;
    #[allow(unused)]
    /// if device is writable
    async fn poll_out(&self) -> bool;
}
