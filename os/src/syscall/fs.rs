//! File and filesystem-related syscalls
use crate::fs::{open_file, OpenFlags};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer};
use crate::task::{current_task, current_user_token};

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

pub fn sys_dup(old_fd: usize) -> isize {
    let task = current_task().unwrap();
    
    let mut inner = task.inner_exclusive_access();

    if let Some(file) = &inner.fd_table[old_fd] {
        let file = file.clone();
        let new_fd = inner.alloc_fd();
        inner.fd_table[new_fd] = Some(file);
        new_fd as isize
    } else {
        -1
    }
}

pub fn sys_dup3(old_fd: usize, new_fd: usize, _flags: u32) -> isize {
    //info!("dup3: old_fd = {}, new_fd = {}", old_fd, new_fd);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if old_fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[old_fd] {
        let file = file.clone();
        if new_fd < inner.fd_table.len() {
            inner.fd_table[new_fd] = Some(file);
        } else {
            inner.fd_table.resize(new_fd + 1, None);
            inner.fd_table[new_fd] = Some(file);
        }
        new_fd as isize
    } else {
        -1
    }
}
