//! (FileWrapper + VfsNodeOps) -> OSInodeInner
//! OSInodeInner -> OSInode
extern crate lwext4_rust;
extern crate virtio_drivers;

use async_trait::async_trait;
use hal::println;
use lwext4_rust::InodeTypes;

use virtio_drivers::device::blk::VirtIOBlk;
use virtio_drivers::transport::mmio::{MmioTransport, VirtIOHeader};
use virtio_drivers::transport::{DeviceType, Transport};


use crate::drivers::block::BLOCK_DEVICE;
use crate::fs::vfs::dentry::global_find_dentry;
use crate::fs::vfs::{Dentry, DentryState, Inode, DCACHE};
use crate::fs::FS_MANAGER;
use crate::utils::{abs_path_to_name, abs_path_to_parent};

use alloc::vec;
use alloc::{format, vec::Vec};
use alloc::string::String;
use alloc::boxed::Box;

use super::{dentry, Ext4Dentry};
use super::inode::Ext4Inode;
use super::disk::Disk;

use crate::fs::{
    vfs::{File, FileInner},
    OpenFlags,
};
use crate::mm::UserBuffer;
use crate::sync::UPSafeCell;
use alloc::sync::Arc;
use bitflags::*;
use lazy_static::*;

use log::*;


/// A wrapper around a filesystem inode
/// to implement File trait atop
pub struct Ext4File {
    readable: bool,
    writable: bool,
    inner: UPSafeCell<FileInner>,
}

unsafe impl Send for Ext4File {}
unsafe impl Sync for Ext4File {}

impl Ext4File {
    /// Construct an Ext4File from a dentry
    pub fn new(readable: bool, writable: bool, dentry: Arc<dyn Dentry>) -> Self {
        Self {
            readable,
            writable,
            inner: UPSafeCell::new(FileInner { offset: 0, dentry }) ,
        }
    }

    /// Read all data inside a inode into vector
    pub fn read_all(&self) -> Vec<u8> {
        let inner = self.inner.exclusive_access();
        let inode = self.dentry().unwrap().inode().unwrap();
        let mut buffer = [0u8; 512];
        let mut v: Vec<u8> = Vec::new();
        loop {
            let len = inode.read_at(inner.offset, &mut buffer).unwrap();
            if len == 0 {
                break;
            }
            inner.offset += len;
            v.extend_from_slice(&buffer[..len]);
        }
        v
    }
}

#[async_trait]
impl File for Ext4File {
    fn inner(&self) -> &FileInner {
        self.inner.exclusive_access()
    }
    fn readable(&self) -> bool {
        self.readable
    }
    fn writable(&self) -> bool {
        self.writable
    }
    async fn read(&self, mut buf: UserBuffer) -> usize {
        let inner = self.inner.exclusive_access();
        let inode = self.dentry().unwrap().inode().unwrap();
        let mut total_read_size = 0usize;
        for slice in buf.buffers.iter_mut() {
            let read_size = inode.read_at(inner.offset, *slice).unwrap();
            if read_size == 0 {
                break;
            }
            inner.offset += read_size;
            total_read_size += read_size;
        }
        total_read_size
    }
    async fn write(&self, buf: UserBuffer) -> usize {
        let inner = self.inner.exclusive_access();
        let inode = self.dentry().unwrap().inode().unwrap();
        let mut total_write_size = 0usize;
        for slice in buf.buffers.iter() {
            let write_size = inode.write_at(inner.offset, *slice).unwrap();
            assert_eq!(write_size, slice.len());
            inner.offset += write_size;
            total_write_size += write_size;
        }
        total_write_size
    }
}


/// helper function: Open file in ext4 fs with flags
/// notice that ext4 file is a abstract
/// it can be reg_file, dir or anything...
/// @path: absolute path
pub fn open_file(path: &str, flags: OpenFlags) -> Option<Arc<Ext4File>> {
    //let root = FS_MANAGER.lock().get("ext4").unwrap().root();
    let (readable, writable) = flags.read_write();

    // get the root dentry and look up for the inode first
    let root_dentry = {
        let dcache = DCACHE.lock();
        Arc::clone(dcache.get("/").unwrap())
    };
    let root_inode = root_dentry.inode().unwrap();
    
    if flags.contains(OpenFlags::CREATE) {
        if let Some(dentry) = root_dentry.find(path) {
            // clear size
            let inode = dentry.inode().unwrap();
            inode.truncate(0).expect("Error when truncating inode");
            Some(Arc::new(Ext4File::new(readable, writable, dentry)))
        } else {
            // create file (todo: now only support root create)
            let inode = root_inode.create(&path, InodeTypes::EXT4_DE_REG_FILE).unwrap();
            let name = abs_path_to_name(&path).unwrap();
            let parent_path = abs_path_to_parent(&path).unwrap();
            let parent_dentry = global_find_dentry(&parent_path);
            assert!(parent_dentry.state() == DentryState::USED);
            let dentry = Ext4Dentry::new(&name, parent_dentry.superblock(), Some(parent_dentry.clone()));
            dentry.set_state(DentryState::USED);
            dentry.set_inode(inode);
            Some(Arc::new(Ext4File::new(readable, writable, dentry)))
        }
    } else {
        if let Some(dentry) = root_dentry.find(path) {
            // get the dentry and it is valid (see dentry::find)
            let inode = dentry.inode().unwrap();
            if flags.contains(OpenFlags::TRUNC) {
                inode.truncate(0).expect("Error when truncating inode");
            }
            Some(Arc::new(Ext4File::new(readable, writable, dentry)))
        } else {
            None
        }
        
    }
}

/// helper function: List all files in the ext4 filesystem
pub fn list_apps() {
    let root_dentry = FS_MANAGER.lock()
    .get("ext4").unwrap()
    .get_sb("/").unwrap()
    .root();
    let root_inode = root_dentry.inode().unwrap();
    println!("/**** APPS ****");
    for app in root_inode.ls() {
        println!("{}", app);
    }
    println!("**************/");
}