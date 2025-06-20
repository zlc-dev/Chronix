//! Implementation of syscalls
//!
//! The single entry point to all system calls, [`syscall()`], is called
//! whenever userspace wishes to perform a system call using the `ecall`
//! instruction. In this case, the processor raises an 'Environment call from
//! U-mode' exception, which is handled as one of the cases in
//! [`crate::trap::trap_handler`].
//!
//! For clarity, each single syscall is implemented as its own function, named
//! `sys_` then the name of the syscall. You can find functions like this in
//! submodules, and you should also implement syscalls this way.

const SYSCALL_GETCWD: usize = 17;
const SYSCALL_DUP: usize = 23;
const SYSCALL_DUP3: usize = 24;
const SYSCALL_FCNTL: usize = 25;
const SYSCALL_IOCTL: usize = 29;
const SYSCALL_MKDIR: usize = 34;
const SYSCALL_UNLINKAT: usize = 35;
const SYSCALL_LINKAT: usize = 37;
const SYSCALL_UMOUNT2: usize = 39;
const SYSCALL_MOUNT: usize = 40;
const SYSCALL_STATFS: usize = 43;
const SYSCALL_FTRUNCATE: usize = 46;
const SYSCALL_FACCESSAT: usize = 48;
const SYSCALL_CHDIR: usize = 49;
const SYSCALL_FCHMODAT: usize = 53;
const SYSCALL_OPENAT: usize = 56;
const SYSCALL_CLOSE: usize = 57;
const SYSCALL_PIPE: usize = 59;
const SYSCALL_GETDENTS: usize = 61;
const SYSCALL_LSEEK: usize = 62;
const SYSCALL_READ: usize = 63;
const SYSCALL_WRITE: usize = 64;
const SYSCALL_READV: usize = 65;
const SYSCALL_WRITEV: usize = 66;
const SYSCALL_PREAD: usize = 67;
const SYSCALL_PWRITE: usize = 68;
const SYSCALL_SENDFILE: usize = 71;
const SYSCALL_PSELECT6: usize = 72;
const SYSCALL_PPOLL: usize = 73;
const SYSCALL_READLINKAT: usize = 78;
const SYSCALL_FSTATAT: usize = 79;
const SYSCALL_FSTAT: usize = 80;
const SYSCALL_SYNC: usize = 81;
const SYSCALL_FSYNC: usize = 82;
const SYSCALL_UTIMENSAT: usize = 88;
const SYSCALL_EXIT: usize = 93;
const SYSCALL_EXIT_GROUP: usize = 94;
const SYSCALL_SET_TID_ADDRESS: usize = 96;
const SYSCALL_FUTEX: usize = 98;
const SYSCALL_SET_ROBUST_LIST: usize = 99;
const SYSCALL_GET_ROBUST_LIST: usize = 100;
const SYSCALL_NANOSLEEP: usize = 101;
const SYSCALL_GETITIMER: usize = 102;
const SYSCALL_SETITIMER: usize = 103;
const SYSCALL_CLOCK_GETTIME: usize = 113;
const SYSCALL_CLOCK_NANOSLEEP: usize = 115;
const SYSCALL_SYSLOG: usize = 116;
const SYSCALL_SCHED_SETSCHEDULER: usize = 119;
const SYSCALL_SCHED_GETSCHEDULER: usize = 120;
const SYSCALL_SCHED_GETPARAM: usize = 121;
const SYSCALL_SCHED_SETAFFINITY: usize = 122;
const SYSCALL_SCHED_GETAFFINITY:usize = 123;
const SYSCALL_YIELD: usize = 124;
const SYSCALL_KILL: usize = 129;
const SYSCALL_TKILL: usize = 130;
const SYSCALL_TGKILL: usize = 131;
const SYSCALL_RT_SIGSUSPEND: usize = 133;
const SYSCALL_RT_SIGACTION: usize = 134;
const SYSCALL_RT_SIGPROCMASK: usize = 135;
const SYSCALL_RT_SIGTIMEDWAIT: usize = 137;
const SYSCALL_RT_SIGRETURN: usize = 139;
const SYSCALL_REBOOT: usize = 142;
const SYSCALL_TIMES: usize = 153;
const SYSCALL_SETPGID: usize = 154;
const SYSCALL_GETPGID: usize = 155;
const SYSCALL_SETSID: usize = 157;
const SYSCALL_UNAME: usize = 160;
const SYSCALL_GETRUSAGE: usize = 165;
const SYSCALL_UMASK: usize = 166;
const SYSCALL_GETTIMEOFDAY: usize = 169;
const SYSCALL_GETPID: usize = 172;
const SYSCALL_GETPPID: usize = 173;
const SYSCALL_GETUID: usize = 174;
const SYSCALL_GETEUID: usize = 175;
const SYSCALL_GETEGID: usize = 177;
const SYSCALL_GETTID: usize = 178;
const SYSCALL_SYSINFO: usize = 179;
const SYSCALL_SHMGET: usize = 194;
const SYSCALL_SHMCTL: usize = 195;
const SYSCALL_SHMAT: usize = 196;
const SYSCALL_SHMDT: usize = 197;
const SYSCALL_SOCKET: usize = 198;
const SYSCALL_SOCKETPAIR: usize = 199;
const SYSCALL_BIND: usize = 200;
const SYSCALL_LISTEN: usize = 201;
const SYSCALL_ACCEPT: usize = 202;
const SYSCALL_CONNECT: usize = 203;
const SYSCALL_GETSOCKNAME: usize = 204;
const SYSCALL_GETPEERNAME: usize = 205;
const SYSCALL_SENDTO: usize = 206;
const SYSCALL_RECVFROM: usize = 207;
const SYSCALL_SETSOCKOPT: usize = 208;
const SYSCALL_GETSOCKOPT: usize = 209;
const SYSCALL_SHUTDOWN: usize = 210;
const SYSCALL_SENDMSG: usize = 211;
const SYSCALL_RECVMSG: usize = 212;
const SYSCALL_BRK: usize = 214;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_MREMAP: usize = 216;
const SYSCALL_CLONE: usize = 220;
const SYSCALL_EXEC: usize = 221;
const SYSCALL_MMAP: usize = 222;
const SYSCALL_MPROTECE: usize = 226;
const SYSCALL_MSYNC: usize = 227;
const SYSCALL_MADSIVE: usize = 233;
const SYSCALL_WAITPID: usize = 260;
const SYSCALL_PRLIMIT64: usize = 261;
const SYSCALL_RENAMEAT2: usize = 276;
const SYSCALL_GETRANDOM: usize = 278;
const SYSCALL_MEMBARRIER: usize = 283;
const SYSCALL_STATX: usize = 291;
const SYSCALL_CLONE3: usize = 435;

pub mod fs;
/// futex
pub mod futex;
pub mod process;
pub mod time;
pub mod signal;
pub mod misc;
pub mod mm;
pub mod io;
/// syscall concerning scheduler
pub mod sche;
/// syscall error code
pub mod sys_error;
/// syscall concerning network
pub mod net;
/// ipc
pub mod ipc;
pub mod reboot;
use alloc::format;
use fatfs::info;
pub use fs::*;
use futex::{sys_futex, sys_get_robust_list, sys_set_robust_list, FUTEX_OWNER_DIED, FUTEX_TID_MASK, FUTEX_WAITERS};
use hal::{addr::VirtAddr, println};
use io::*;
use ipc::sysv::{sys_shmat, sys_shmctl, sys_shmdt, sys_shmget};
use misc::*;
use mm::{sys_mmap, sys_mprotect, sys_mremap, sys_munmap};
use net::*;
pub use process::*;
pub use time::*;
pub use signal::*;
pub use sche::*;
pub use reboot::*;
pub use self::sys_error::SysError;
use crate::{fs::RenameFlags, mm::UserPtr, signal::{SigAction, SigSet}, task::current_task, timer::ffi::{TimeVal, Tms}, utils::{timer::TimerGuard, SendWrapper}};
/// The result of a syscall, either Ok(return value) or Err(error code)
pub type SysResult = Result<isize, SysError>;

/// handle syscall exception with `syscall_id` and other arguments
pub async fn syscall(syscall_id: usize, args: [usize; 6]) -> isize {
    log::debug!("task {}, syscall id: {}", current_task().unwrap().tid() ,syscall_id);
    let result = match syscall_id { 
        SYSCALL_GETCWD => sys_getcwd(args[0] as usize, args[1] as usize),
        SYSCALL_DUP => sys_dup(args[0] as usize),
        SYSCALL_DUP3 => sys_dup3(args[0] as usize, args[1] as usize, args[2] as u32),
        SYSCALL_FCNTL => sys_fnctl(args[0], args[1] as isize, args[2]),
        SYSCALL_IOCTL => sys_ioctl(args[0], args[1], args[2]),
        SYSCALL_OPENAT => sys_openat(args[0] as isize , args[1] as *const u8, args[2] as u32, args[3] as u32),
        SYSCALL_MKDIR => sys_mkdirat(args[0] as isize, args[1] as *const u8, args[2] as usize),
        SYSCALL_UNLINKAT => sys_unlinkat(args[0] as isize, args[1] as *const u8, args[3] as i32),
        SYSCALL_LINKAT => sys_linkat(args[0] as isize, args[1] as *const u8, args[2] as isize, args[3] as *const u8, args[4] as i32),
        SYSCALL_MOUNT => sys_mount(args[0] as *const u8, args[1] as *const u8, args[2] as *const u8, args[3] as u32, args[4] as usize),
        SYSCALL_STATFS => sys_statfs(args[0], args[1]),
        SYSCALL_FTRUNCATE => sys_ftruncate(args[0], args[1]),
        SYSCALL_FACCESSAT => sys_faccessat(args[0] as isize, args[1] as *const u8, args[2], args[3] as i32),
        SYSCALL_UMOUNT2 => sys_umount2(args[0] as *const u8, args[1] as u32),
        SYSCALL_CHDIR => sys_chdir(args[0] as *const u8),
        SYSCALL_FCHMODAT => sys_fchmodat(),
        SYSCALL_CLOSE => sys_close(args[0]),
        SYSCALL_PIPE => sys_pipe2(args[0] as *mut i32, args[1] as u32),
        SYSCALL_GETDENTS => sys_getdents64(args[0], args[1], args[2]),
        SYSCALL_LSEEK => sys_lseek(args[0], args[1] as isize, args[2]),
        SYSCALL_READ => sys_read(args[0], args[1] , args[2]).await,
        SYSCALL_WRITE => sys_write(args[0], args[1] , args[2]).await,
        SYSCALL_READV => sys_readv(args[0], args[1], args[2]).await,
        SYSCALL_WRITEV => sys_writev(args[0], args[1], args[2]).await,
        SYSCALL_PREAD => sys_pread(args[0], args[1], args[2], args[3]).await,
        SYSCALL_PWRITE => sys_pwrite(args[0], args[1], args[2], args[3]).await,
        SYSCALL_SENDFILE => sys_sendfile(args[0], args[1], args[2], args[3]).await,
        SYSCALL_PPOLL => sys_ppoll(args[0], args[1], args[2], args[3]).await,
        SYSCALL_PSELECT6 => sys_pselect6(args[0] as i32, args[1], args[2], args[3], args[4], args[5]).await,
        SYSCALL_READLINKAT => sys_readlinkat(args[0] as isize, args[1] as *const u8, args[2], args[3]),
        SYSCALL_FSTATAT => sys_fstatat(args[0] as isize, args[1] as *const u8, args[2], args[3] as i32),
        SYSCALL_FSTAT => sys_fstat(args[0], args[1]),
        SYSCALL_UTIMENSAT => sys_utimensat(args[0] as isize, args[1] as *const u8, args[2], args[3] as i32),
        SYSCALL_EXIT => sys_exit(args[0] as i32),
        SYSCALL_SET_TID_ADDRESS => sys_set_tid_address(args[0]),
        SYSCALL_EXIT_GROUP => sys_exit_group(args[0] as i32),
        SYSCALL_FUTEX => sys_futex(args[0], args[1] as _, args[2] as _, SendWrapper(args[3] as _), args[4], args[5] as _).await,
        SYSCALL_SET_ROBUST_LIST => sys_set_robust_list(args[0] as _, args[1]),
        SYSCALL_GET_ROBUST_LIST => sys_get_robust_list(args[0] as _, args[1] as _, args[2] as _),
        SYSCALL_NANOSLEEP => sys_nanosleep(args[0].into(),args[1].into()).await,
        SYSCALL_GETITIMER => sys_getitimer(args[0], args[1]),
        SYSCALL_SETITIMER => sys_setitimer(args[0],args[1],args[2]),
        SYSCALL_CLOCK_GETTIME => sys_clock_gettime(args[0], args[1]),
        SYSCALL_CLOCK_NANOSLEEP => sys_clock_nanosleep(args[0], args[1], args[2], args[3]).await,
        SYSCALL_SYSLOG => sys_syslog(args[0], args[1], args[2]),
        SYSCALL_SCHED_SETAFFINITY => sys_sched_setaffinity(args[0] , args[1] , args[2] ),
        SYSCALL_SCHED_GETAFFINITY => sys_sched_getaffinity(args[0] , args[1] , args[2] ),
        SYSCALL_SCHED_GETSCHEDULER => sys_sched_getscheduler(),
        SYSCALL_SCHED_SETSCHEDULER => sys_sched_setscheduler(),
        SYSCALL_SCHED_GETPARAM => sys_sched_getparam(),
        SYSCALL_YIELD => sys_yield().await,
        SYSCALL_KILL => sys_kill(args[0] as isize, args[1] as i32),
        SYSCALL_TKILL => sys_tkill(args[0] as isize, args[1] as i32),
        SYSCALL_TGKILL => sys_tgkill( args[0] as isize, args[1] as isize, args[2] as i32),
        SYSCALL_RT_SIGSUSPEND => sys_rt_sigsuspend(args[0]).await,
        SYSCALL_RT_SIGACTION => sys_rt_sigaction(args[0] as i32, args[1] as *const SigAction, args[2] as *mut SigAction),
        SYSCALL_RT_SIGPROCMASK => sys_rt_sigprocmask(args[0] as i32, args[1] as *const u32, args[2] as *mut SigSet),
        SYSCALL_RT_SIGRETURN => sys_rt_sigreturn(),
        SYSCALL_RT_SIGTIMEDWAIT => sys_rt_sigtimedwait(args[0] , args[1] , args[2] ).await,
        SYSCALL_REBOOT => sys_reboot(args[0] as _, args[0] as _, args[0] as _, args[0]).await,
        SYSCALL_TIMES => sys_times(args[0] as *mut Tms),
        SYSCALL_UNAME => sys_uname(args[0]),
        SYSCALL_UMASK => sys_umask(args[0] as i32),
        SYSCALL_GETTIMEOFDAY => sys_gettimeofday(args[0] as *mut TimeVal),
        SYSCALL_GETPID => sys_getpid(),
        SYSCALL_GETPPID => sys_getppid(),
        SYSCALL_GETUID => sys_getuid(),
        SYSCALL_GETEUID => sys_geteuid(),
        SYSCALL_GETEGID => sys_getegid(),
        SYSCALL_GETTID => sys_gettid(),
        SYSCALL_SETSID => sys_setsid(),
        SYSCALL_SYSINFO => sys_sysinfo(args[0]),
        SYSCALL_SHMGET => sys_shmget(args[0] as _, args[1] as _, args[2] as _),
        SYSCALL_SHMCTL => sys_shmctl(args[0] as _, args[1] as _, UserPtr::new(args[2] as *mut _)),
        SYSCALL_SHMAT => sys_shmat(args[0] as _, VirtAddr::from(args[1]), args[2] as _),
        SYSCALL_SHMDT => sys_shmdt(VirtAddr::from(args[0])),
        SYSCALL_SETPGID => sys_setpgid(args[0], args[1]),
        SYSCALL_GETPGID => sys_getpgid(args[0]),
        SYSCALL_CLONE => sys_clone(args[0] as u64, args[1].into(), args[2].into(), args[3].into(), args[4].into()),
        SYSCALL_CLONE3 => sys_clone3(args[0], args[1]),
        SYSCALL_WAITPID => sys_waitpid(args[0] as isize, args[1], args[2] as i32).await,
        SYSCALL_PRLIMIT64 => sys_prlimit64(args[0], args[1] as i32, args[2], args[3]),
        SYSCALL_GETRUSAGE => sys_getrusage(args[0] as i32, args[1]),
        SYSCALL_EXEC => sys_execve(args[0] , args[1], args[2]).await,
        SYSCALL_BRK => sys_brk(VirtAddr::from(args[0])),
        SYSCALL_MUNMAP => sys_munmap(VirtAddr::from(args[0]), args[1]),
        SYSCALL_MMAP => sys_mmap(VirtAddr::from(args[0]), args[1], args[2] as i32, args[3] as i32, args[4], args[5]),
        SYSCALL_MREMAP => sys_mremap(VirtAddr::from(args[0]), args[1], args[2], args[3] as i32, args[4]),
        SYSCALL_RENAMEAT2 => sys_renameat2(args[0] as isize, args[1] as *const u8, args[2] as isize, args[3] as *const u8, args[4] as i32),
        SYSCALL_GETRANDOM => sys_getrandom(args[0], args[1], args[2]),
        SYSCALL_STATX => sys_statx(args[0] as _, args[1] as _, args[2] as _, args[3] as _, args[4].into()),
        SYSCALL_SOCKET => sys_socket(args[0], args[1] as i32, args[2]),
        SYSCALL_SOCKETPAIR => sys_socketpair(args[0], args[1],  args[2], args[3]),
        SYSCALL_BIND => sys_bind(args[0], args[1], args[2]),
        SYSCALL_LISTEN => sys_listen(args[0], args[1]),
        SYSCALL_ACCEPT => sys_accept(args[0], args[1], args[2]).await,
        SYSCALL_CONNECT => sys_connect(args[0], args[1], args[2]).await,
        SYSCALL_GETSOCKNAME => sys_getsockname(args[0], args[1], args[2]),
        SYSCALL_GETPEERNAME => sys_getpeername(args[0], args[1], args[2]),
        SYSCALL_SENDTO => sys_sendto(args[0], args[1] ,  args[2], args[3], args[4], args[5]).await,
        SYSCALL_RECVFROM => sys_recvfrom(args[0], args[1] , args[2], args[3], args[4], args[5]).await,
        SYSCALL_SETSOCKOPT => sys_setsockopt(args[0], args[1], args[2], args[3], args[4]),
        SYSCALL_GETSOCKOPT => sys_getsockopt(args[0], args[1], args[2], args[3], args[4]),
        SYSCALL_SHUTDOWN => sys_shutdown(args[0],  args[1]),
        SYSCALL_SENDMSG => sys_sendmsg(args[0], args[1], args[2]).await,
        SYSCALL_RECVMSG => sys_recvmsg(args[0], args[1], args[2]).await,
        SYSCALL_MPROTECE => sys_mprotect(args[0].into(), args[1], args[2] as _),
        SYSCALL_MADSIVE =>  sys_temp(),
        SYSCALL_SYNC => sys_temp(),
        SYSCALL_FSYNC => sys_temp(),
        SYSCALL_MSYNC => sys_temp(),
        SYSCALL_MEMBARRIER => sys_temp(),
        _ => { 
            log::warn!("Unsupported syscall_id: {}", syscall_id);
            Err(SysError::ENOSYS)
        }
    };
    match result {
        Ok(ret ) => {
            ret
        }
        Err(err) => {
            -err.code() 
        }
    }
}
/// do nothing
pub fn sys_temp() -> SysResult {
    Ok(0)
}
