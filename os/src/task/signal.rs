//! task signal related implement

use core::{future::Future, pin::Pin, task::{Context, Poll}};

use alloc::sync::Arc;
use fatfs::info;
use hal::{addr::VirtAddr, println, signal::{sigreturn_trampoline_addr, UContext, UContextHal}, trap::TrapContextHal};

use crate::{mm::{vm::UserVmSpaceHal, UserPtrRaw}, signal::{KSigAction, LinuxSigInfo, SigAction, SigActionFlag, SigHandler, SigInfo, SigSet, SIGCHLD, SIGKILL, SIGSTOP}, task::INITPROC_PID, trap::trap_return};

use super::task::TaskControlBlock;


/// for the signal mechanism
impl TaskControlBlock {
    /// once the leader thread change the sig action
    /// all its follower should change
    pub fn set_sigaction(&self, signo: usize, sigaction: KSigAction) {
        //info!("[TCB] sync all child thread sigaction");
        // self.sig_manager.lock().set_sigaction(signo, sigaction);
        self.with_mut_thread_group(|tg| {
            for thread in tg.iter() {
                thread.sig_manager.lock().set_sigaction(signo, sigaction);
            }
        })
    }
    /// set self's wake up signals
    /// when these signals arrive it should wake itself up
    pub fn set_wake_up_sigs(&self, sigs: SigSet) {
        assert!(self.is_interruptable() || self.is_stopped());
        self.with_mut_sig_manager(|manager| {
            manager.wake_sigs = sigs | SigSet::SIGKILL | SigSet::SIGSTOP
        })
    }
    /// receive function at TCB level
    /// as we may need to wake up a task when wake up signal come
    pub fn recv_sigs(&self, sig: SigInfo) {
        log::info!("[TCB]: tid {} recv signo {:?}", self.gettid(), sig);
        self.with_mut_sig_manager(|manager| {
            manager.receive(sig);
            if manager.wake_sigs.contain_sig(sig.si_signo) && self.is_interruptable() {
                //info!("[TCB]: tid {} has been wake up", self.gettid());
                self.wake();
            } 
            /* else if manager.wake_sigs.contain_sig(sig.si_signo) && self.is_zombie() {
                log::info!("[TCB]: wake up tid {} to finish its handle zombie", self.gettid());
                self.wake();
            } */
        });
    }
    /// Unix has two types of signal: Process level and Thread level
    /// in Process-level, all threads in the same process share the same signal mask
    pub fn recv_sigs_process_level(&self, sig_info: SigInfo) {
        log::info!("[TCB::recv_sigs_process_level]: tid {} recv signo {} at process level",self.tid(),sig_info.si_signo);
        self.with_mut_thread_group(|tg| {
            let mut signal_delivered = false;
            for thread in tg.iter() {
                if thread.sig_manager.lock().blocked_sigs.contain_sig(sig_info.si_signo) {
                    continue;
                }
                thread.recv_sigs(sig_info);
                signal_delivered = true;
                break;
            }
            if !signal_delivered {
                let task = tg.iter().next().unwrap();
                task.recv_sigs(sig_info);
            }
        })
    }

    /// child process notify parent
    /// send SIGCHLD signal to parent
    /// Let a parent know about the death of a child.
    /// TODOS: should closer to linux design; si_code;
    pub fn notify_parent(self: &Arc<Self>) {
        
        if let Some(parent) = self.parent() {
            if let Some(parent) = parent.upgrade() {
                // log::info!("[TCB] task {} notify parent", self.gettid());
                parent.recv_sigs_process_level(
                    SigInfo { si_signo: SIGCHLD, si_code: SigInfo::CLD_EXITED, si_pid: Some(self.pid()) }
                );
            }else {
                log::error!("no parent !");
            }
        }
    }
    
    /// signal manager should check the signal queue
    /// before a task return form kernel to user
    /// and make correspond handle action
    /// if return true, need to restart the system call if it returns SIGINTR
    pub fn check_and_handle(self: &Arc<Self>, mut is_intr: bool, old_a0: usize) {
        loop {
            let mut sig_manager = self.sig_manager.lock();
            if let Some(sig) = sig_manager.dequeue_one() {
                // handle a signal
                assert!(sig.si_signo != 0);
                let sig_action = sig_manager.sig_handler[sig.si_signo];
                // log::info!("[check_and_handle] task {} action {:?}", self.tid(), sig_action);
                let sa_flags = SigActionFlag::from_bits_truncate(sig_action.sa.sa_flags);
                
                let trap_cx = self.trap_context.exclusive_access();
                
                if sa_flags.contains(SigActionFlag::SA_RESTART) && is_intr {
                    *trap_cx.sepc() -= 4;
                    trap_cx.set_ret_nth(0, old_a0);
                    is_intr = false
                }

                if sig_action.is_user {
                    let old_blocked_sigs = sig_manager.blocked_sigs; // save for later restore
                    if !sa_flags.contains(SigActionFlag::SA_NODEFER) {
                        sig_manager.blocked_sigs.add_sig(sig.si_signo);
                    };
                    sig_manager.blocked_sigs |= sig_action.sa.sa_mask[0];
                    // save fx state
                    trap_cx.fx_encounter_signal();
                    // push the current Ucontext into user stack
                    // (todo) notice that user may provide signal stack
                    // but now we dont support this flag
                    let sp = *trap_cx.sp();
                    let mut new_sp = sp - size_of::<UContext>();
                    let ucontext = UContext::save_current_context(old_blocked_sigs.bits(), trap_cx);
                    let dst = 
                        UserPtrRaw::new(new_sp as *mut UContext).ensure_write(&mut self.get_vm_space().lock()).unwrap();
                    // println!("copy_out to {:#x}", new_sp);
                    // copy_out(&mut self.get_vm_space().lock(), VirtAddr(new_sp), ucontext_bytes);
                    dst.write(ucontext);
                    self.set_sig_ucontext_ptr(new_sp);
                    
                    // the first argument of every signal handlers is signo
                    trap_cx.set_arg_nth(0, sig.si_signo);

                    // SA_SIGINFO flag is set, need to pass more args
                    // void (*sa_sigaction)(int, siginfo_t *, void *ucontext)
                    if sa_flags.contains(SigActionFlag::SA_SIGINFO) {
                        log::warn!("using SA_SIGINFO flags, pass more arguments");
                        // the second argument
                        trap_cx.set_arg_nth(2, new_sp);
                        // the third argument
                        let mut siginfo_v = LinuxSigInfo::default();
                        siginfo_v.si_signo = sig.si_signo as _;
                        siginfo_v.si_code = sig.si_code;
                        siginfo_v._pad[1] = sig.si_pid.unwrap_or(0) as i32;
                        new_sp -= size_of::<LinuxSigInfo>();
                        let dst = 
                            UserPtrRaw::new(new_sp as *mut LinuxSigInfo).ensure_write(&mut self.get_vm_space().lock()).unwrap();
                        dst.write(siginfo_v);
                        trap_cx.set_arg_nth(1, new_sp);
                    }

                    // set the current trap cx sepc to reach user handler
                    // log::info!("set signal handler sepc: {:x}", sig_action.sa.sa_handler as *const usize as usize);
                    *trap_cx.sepc() = sig_action.sa.sa_handler as *const usize as usize;
                    // sp
                    *trap_cx.sp() = new_sp;
                    // ra: when user signal handler ended, return to sigreturn_trampoline
                    // which calls sys_sigreturn
                    *trap_cx.ra() = sigreturn_trampoline_addr();
                    *trap_cx.tp() = ucontext.uc_mcontext.get_tp();

                    break;
                } else {
                    drop(sig_manager);
                    let handler = unsafe {
                        core::mem::transmute::<*const (), SigHandler>(
                            sig_action.sa.sa_handler as *const (),
                        )
                    };
                    handler(sig.si_signo as i32);
                }
            } else {
                break;
            }
        }
    }
}

/// the future that check if recv expect signal
pub struct IntrBySignalFuture {
    /// the task needed to check
    pub task: Arc<TaskControlBlock>,
    /// current signal mask
    pub mask: SigSet,
}

impl Future for IntrBySignalFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let has_signal = !(self.task.sig_manager.lock().bitmap & !self.mask).is_empty();
        if has_signal {
            log::warn!("[IntrBySignalFuture] received interupt signal");
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}