//
// lib.rs
// Copyright (C) 2017 Adrian Perez <aperez@igalia.com>
// Distributed under terms of the MIT license.
//

extern crate libicecc_sys as sys;
extern crate libc;

use std::convert::AsRef;
use std::ffi::{ CStr, CString };
use std::fmt;
use std::rc::Rc;
use libc::{ c_char, c_int, c_void };


#[derive(Debug, Eq, PartialEq)]
pub enum Language {
    C,
    CPlusPlus,
    ObjectiveC,
    Custom,
}

impl From<sys::CompileJobLanguage> for Language
{
    fn from(lang: sys::CompileJobLanguage) -> Self {
        match lang {
            sys::CompileJobLanguage::C => Language::C,
            sys::CompileJobLanguage::CXX => Language::CPlusPlus,
            sys::CompileJobLanguage::OBJC => Language::ObjectiveC,
            sys::CompileJobLanguage::CUSTOM => Language::Custom,
        }
    }
}

impl From<Language> for sys::CompileJobLanguage
{
    fn from(lang: Language) -> Self {
        match lang {
            Language::C => sys::CompileJobLanguage::C,
            Language::CPlusPlus => sys::CompileJobLanguage::CXX,
            Language::ObjectiveC => sys::CompileJobLanguage::OBJC,
            Language::Custom => sys::CompileJobLanguage::CUSTOM,
        }
    }
}


// TODO: Make this trait private.
pub trait AsPtr {
    type Output;
    fn as_ptr(&self) -> *mut Self::Output;
}


macro_rules! implement_ptrs {
    ($(($name:ident $sysfree:ident))+) => {
        $(
            pub struct $name(pub *mut $crate::sys::$name);

            impl Drop for $name {
                fn drop(&mut self) {
                    unsafe { $crate::sys::$sysfree(self.0) };
                }
            }

            impl AsPtr for $name {
                type Output = $crate::sys::$name;
                fn as_ptr(&self) -> *mut Self::Output { 
                    self.0
                }
            }
        )+
    }
}


mod ptr {
    use super::AsPtr;

    implement_ptrs! {
        (CompileJob compile_job_free)
        (DiscoverSched discover_sched_free)
        (MsgChannel msg_channel_free)
        (Msg msg_free)
    }

    impl Msg {
        pub fn message_type(&self) -> super::sys::MsgType {
            unsafe { super::sys::msg_get_type(self.as_ptr()) }
        }
    }
}



#[derive(Clone)]
pub struct ScheduleDiscoverer {
    sd: Rc<ptr::DiscoverSched>,
}

impl ScheduleDiscoverer
{
    pub fn new<'f, T: Into<Option<&'f String>>>(netname: T) -> Self {
        match netname.into() {
            None => Self {
                sd: Rc::new(ptr::DiscoverSched(unsafe {
                    sys::discover_sched_new(std::ptr::null())
                }))
            },
            Some(name) => {
                let s = CString::new(name.as_bytes()).unwrap();
                Self {
                    sd: Rc::new(ptr::DiscoverSched(unsafe {
                        sys::discover_sched_new(s.as_ptr())
                    }))
                }
            },
        }
    }

    pub fn new_with_options(netname: &str, scheduler: &str, timeout: u32) -> Self {
        let c_netname = CString::new(netname).unwrap();
        let c_scheduler = CString::new(scheduler).unwrap();
        Self {
            sd: Rc::new(ptr::DiscoverSched(unsafe {
                sys::discover_sched_new_with_options(c_netname.as_ptr(),
                                                     c_scheduler.as_ptr(),
                                                     timeout as c_int)
            }))
        }
    }

    pub fn timed_out(&mut self) -> bool {
        unsafe { sys::discover_sched_timed_out(self.sd.as_ptr()) }
    }

    pub fn listen_fd(&self) -> c_int {
        unsafe { sys::discover_sched_listen_fd(self.sd.as_ptr()) }
    }
    
    pub fn connect_fd(&self) -> c_int {
        unsafe { sys::discover_sched_connect_fd(self.sd.as_ptr()) }
    }

    pub fn try_get_scheduler(&mut self) -> Option<MessageChannel> {
        let ptr = unsafe { sys::discover_sched_try_get_scheduler(self.sd.as_ptr()) };
        if ptr.is_null() {
            None
        } else {
            Some(MessageChannel::from_raw_ptr(ptr))
        }
    }
}


#[derive(Clone)]
pub struct MessageChannel {
    mc: Rc<ptr::MsgChannel>,
}

impl MessageChannel
{
    fn from_raw_ptr(ptr: *mut sys::MsgChannel) -> Self {
        assert!(!ptr.is_null());
        Self {
            mc: Rc::new(ptr::MsgChannel(ptr))
        }
    }

    pub fn fd(&self) -> c_int {
        unsafe { sys::msg_channel_fd(self.mc.as_ptr()) }
    }

    pub fn bulk_transfer(&mut self) {
        unsafe { sys::msg_channel_set_bulk_transfer(self.mc.as_ptr()) };
    }

    pub fn read_a_bit(&mut self) -> bool {
        unsafe { sys::msg_channel_read_a_bit(self.mc.as_ptr()) }
    }

    pub fn eof(&self) -> bool {
        unsafe { sys::msg_channel_at_eof(self.mc.as_ptr()) }
    }

    pub fn is_text_based(&self) -> bool {
        unsafe { sys::msg_channel_is_text_based(self.mc.as_ptr()) }
    }

    pub fn has_message(&self) -> bool {
        unsafe { sys::msg_channel_has_msg(self.mc.as_ptr()) }
    }

    pub fn recv(&mut self, timeout: Option<u32>) -> Option<Message> {
        let ptr = match timeout {
            None => unsafe { sys::msg_channel_get_msg(self.mc.as_ptr()) },
            Some(t) => unsafe { sys::msg_channel_get_msg_with_timeout(self.mc.as_ptr(), t as c_int) },
        };
        if ptr.is_null() {
            None
        } else {
            Some(Message::from_raw_ptr(ptr))
        }
    }

    pub fn send<M: AsRef<Message>>(&mut self, message: M) {
        let ptr = message.as_ref().as_raw_ptr();
        unsafe { sys::msg_send_to_channel(ptr, self.mc.as_ptr()) }
    }
}


macro_rules! accessor_simple {
    (($t:ty) $fget:ident $sysfget:ident $fset:ident $sysfset:ident) => {
        pub fn $fget(&self) -> $t {
            unsafe { $crate::sys::$sysfget(self.as_ptr()) }.into()
        }
        pub fn $fset(&mut self, value: $t) {
            let v = value.into();
            unsafe { $crate::sys::$sysfset(self.as_ptr(), v) };
        }
    }
}

macro_rules! accessor_string {
    ($fget:ident $sysfget:ident $fset:ident $sysfset:ident) => {
        pub fn $fget(&self) -> String {
            unsafe {
                let ptr = $crate::sys::$sysfget(self.as_ptr());
                assert_ne!(ptr, 0 as *mut c_char);
                let s = String::from_utf8(CStr::from_ptr(ptr).to_bytes().to_vec()).unwrap();
                libc::free(ptr as *mut c_void);
                s
            }
        }

        pub fn $fset(&mut self, value: &str) {
            let cs = CString::new(value).unwrap();
            unsafe { $crate::sys::$sysfset(self.as_ptr(), cs.as_ptr()) };
        }
    }
}

macro_rules! accessor_dispatch {
    ((String $( $ids:ident )+)) => {
        accessor_string! { $( $ids )+ }
    };
    (($tname:ident $( $ids:ident )+)) => {
        accessor_simple! { ($tname) $( $ids )+ }
    };
}

macro_rules! accessors {
    ($(($type:ident $($ids:ident)+))+) => {
        $( accessor_dispatch! { ($type $($ids)+) } )+
    }
}


pub mod msg {
    use super::*;

    pub trait Base: AsPtr {
        fn as_raw_ptr(&self) -> *mut sys::Msg {
            self.as_ptr() as *mut sys::Msg
        }

        fn as_const_ptr(&self) -> *const Self::Output {
            self.as_raw_ptr() as *const Self::Output
        }

        fn fill_from_channel(&mut self, chan: &mut MessageChannel) {
            unsafe { sys::msg_fill_from_channel(self.as_raw_ptr(), chan.mc.as_ptr()) }
        }

        fn send_to_channel(&self, chan: &mut MessageChannel) {
            unsafe { sys::msg_send_to_channel(self.as_raw_ptr(), chan.mc.as_ptr()) }
        }
    }

    macro_rules! implement_messages {
        ($($name:ident => $sysname:ident { $( $rest:tt )* })+) => {
            $(
                pub struct $name {
                    msg: Rc<ptr::Msg>,
                }

                impl Base for $name {}

                impl AsPtr for $name {
                    type Output = sys::$sysname;
                    fn as_ptr(&self) -> *mut Self::Output {
                        self.msg.as_ptr() as *mut Self::Output
                    }
                }

                impl fmt::Debug for $name {
                    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        write!(f, "msg::{}", stringify!($name))
                    }
                }

                impl From<ptr::Msg> for $name {
                    fn from(msg: ptr::Msg) -> Self {
                        Self {
                            msg: Rc::new(msg)
                        }
                    }
                }

                impl From<$name> for Message {
                    fn from(msg: $name) -> Message {
                        Message::$name(msg)
                    }
                }

                impl $name {
                    fn from_raw_ptr(ptr: *mut <$name as AsPtr>::Output) -> Self {
                        $name::from(ptr::Msg(ptr as *mut $crate::sys::Msg))
                    }

                    $( $rest )*
                }
            )+
        }
    }

    implement_messages!{
        Ping => PingMsg {
            pub fn new() -> Self {
                Ping::from_raw_ptr(unsafe { sys::msg_ping_new() })
            }
        }

        End => EndMsg {}
        GetNativeEnv => GetNativeEnvMsg {}
        NativeEnv => UseNativeEnvMsg {}
        GetCS => UseCSMsg {}
        UseCS => UseCSMsg {}
        CompileFile => CompileFileMsg {}
        FileChunk => FileChunkMsg {}
        CompileResult => CompileResultMsg {}
        JobBegin => JobBeginMsg {}
        JobDone => JobDoneMsg {}
        LocalJobBegin => JobLocalBeginMsg {}
        LocalJobDone => JobLocalDoneMsg {}
        Login => LoginMsg {}
        ConfCS => ConfCSMsg {}
        Stats => StatsMsg {}
        EnvTransfer => EnvTransferMsg {}
        InternalStatus => GetInternalStatusMsg {}
        MonitorLogin => MonLoginMsg {}
        MonitorGetCS => MonGetCSMsg {}
        MonitorJobBegin => MonJobBeginMsg {}

        MonitorJobDone => MonJobDoneMsg {
            pub fn job_id(&self) -> u32 {
                unsafe { sys::msg_job_done_id(self.as_ptr() as *mut sys::JobDoneMsg) }
            }
        }

        MonitorLocalJobBegin => MonLocalJobBeginMsg {
            accessors! {
                (u32
                    job_id msg_mon_local_job_begin_job_id
                    set_job_id msg_mon_local_job_begin_set_job_id)
                (String
                    filename msg_mon_local_job_begin_file
                    set_filename msg_mon_local_job_begin_set_file)
            }
        }

        MonitorStats => MonStatsMsg {
            accessors! {
                (u32
                    host_id msg_mon_stats_host_id
                    set_host_id msg_mon_stats_set_host_id)
                (String
                    message msg_mon_stats_message
                    set_message msg_mon_stats_set_message)
            }
        }

        Text => TextMsg {}
        StatusText => StatusTextMsg {}
        VerifyEnv => VerifyEnvMsg {}
        VerifyEnvResult => VerifyEnvResultMsg {}
        BlacklistHostEnv => BlacklistHostEnvMsg {}
    }
}


#[derive(Debug)]
pub enum Message {
    Ping(msg::Ping),
    End(msg::End),
    GetNativeEnv(msg::GetNativeEnv),
    NativeEnv(msg::NativeEnv),
    GetCS(msg::GetCS),
    UseCS(msg::UseCS),
    CompileFile(msg::CompileFile),
    FileChunk(msg::FileChunk),
    CompileResult(msg::CompileResult),
    JobBegin(msg::JobBegin),
    JobDone(msg::JobDone),
    LocalJobBegin(msg::LocalJobBegin),
    LocalJobDone(msg::LocalJobDone),
    Login(msg::Login),
    ConfCS(msg::ConfCS),
    Stats(msg::Stats),
    EnvTransfer(msg::EnvTransfer),
    InternalStatus(msg::InternalStatus),
    MonitorLogin(msg::MonitorLogin),
    MonitorGetCS(msg::MonitorGetCS),
    MonitorJobBegin(msg::MonitorJobBegin),
    MonitorJobDone(msg::MonitorJobDone),
    MonitorLocalJobBegin(msg::MonitorLocalJobBegin),
    MonitorStats(msg::MonitorStats),
    Text(msg::Text),
    StatusText(msg::StatusText),
    VerifyEnv(msg::VerifyEnv),
    VerifyEnvResult(msg::VerifyEnvResult),
    BlacklistHostEnv(msg::BlacklistHostEnv),
}


impl Message {
    fn from_raw_ptr(ptr: *mut sys::Msg) -> Self {
        let msg = ptr::Msg(ptr);
        match msg.message_type() {
            sys::MsgType::M_UNKNOWN => panic!("M_UNKNOWN messages are unused"),
            sys::MsgType::M_PING => Message::Ping(msg.into()),
            sys::MsgType::M_END => Message::End(msg.into()),
            sys::MsgType::M_TIMEOUT => panic!("M_TIMEOUT messages are unused"),
            sys::MsgType::M_GET_NATIVE_ENV => Message::GetNativeEnv(msg.into()),
            sys::MsgType::M_NATIVE_ENV => Message::NativeEnv(msg.into()),
            sys::MsgType::M_GET_CS => Message::GetCS(msg.into()),
            sys::MsgType::M_USE_CS => Message::UseCS(msg.into()),
            sys::MsgType::M_COMPILE_FILE => Message::CompileFile(msg.into()),
            sys::MsgType::M_FILE_CHUNK => Message::FileChunk(msg.into()),
            sys::MsgType::M_COMPILE_RESULT => Message::CompileResult(msg.into()),
            sys::MsgType::M_JOB_BEGIN => Message::JobBegin(msg.into()),
            sys::MsgType::M_JOB_DONE => Message::JobDone(msg.into()),
            sys::MsgType::M_JOB_LOCAL_BEGIN => Message::LocalJobBegin(msg.into()),
            sys::MsgType::M_JOB_LOCAL_DONE => Message::LocalJobDone(msg.into()),
            sys::MsgType::M_LOGIN => Message::Login(msg.into()),
            sys::MsgType::M_CS_CONF => Message::ConfCS(msg.into()),
            sys::MsgType::M_STATS => Message::Stats(msg.into()),
            sys::MsgType::M_TRANSFER_ENV => Message::EnvTransfer(msg.into()),
            sys::MsgType::M_GET_INTERNALS => Message::InternalStatus(msg.into()),
            sys::MsgType::M_MON_LOGIN => Message::MonitorLogin(msg.into()),
            sys::MsgType::M_MON_GET_CS => Message::MonitorGetCS(msg.into()),
            sys::MsgType::M_MON_JOB_BEGIN => Message::MonitorJobBegin(msg.into()),
            sys::MsgType::M_MON_JOB_DONE => Message::MonitorJobDone(msg.into()),
            sys::MsgType::M_MON_LOCAL_JOB_BEGIN => Message::MonitorLocalJobBegin(msg.into()),
            sys::MsgType::M_MON_STATS => Message::MonitorStats(msg.into()),
            sys::MsgType::M_TEXT => Message::Text(msg.into()),
            sys::MsgType::M_STATUS_TEXT => Message::StatusText(msg.into()),
            sys::MsgType::M_VERIFY_ENV => Message::VerifyEnv(msg.into()),
            sys::MsgType::M_VERIFY_ENV_RESULT => Message::VerifyEnvResult(msg.into()),
            sys::MsgType::M_BLACKLIST_HOST_ENV => Message::BlacklistHostEnv(msg.into()),
        }
    }

    fn as_raw_ptr(&self) -> *mut sys::Msg {
        use msg::Base;
        match *self {
            Message::Ping(ref m) => m.as_raw_ptr(),
            Message::End(ref m) => m.as_raw_ptr(),
            Message::GetNativeEnv(ref m) => m.as_raw_ptr(),
            Message::NativeEnv(ref m) => m.as_raw_ptr(),
            Message::GetCS(ref m) => m.as_raw_ptr(),
            Message::UseCS(ref m) => m.as_raw_ptr(),
            Message::CompileFile(ref m) => m.as_raw_ptr(),
            Message::FileChunk(ref m) => m.as_raw_ptr(),
            Message::CompileResult(ref m) => m.as_raw_ptr(),
            Message::JobBegin(ref m) => m.as_raw_ptr(),
            Message::JobDone(ref m) => m.as_raw_ptr(),
            Message::LocalJobBegin(ref m) => m.as_raw_ptr(),
            Message::LocalJobDone(ref m) => m.as_raw_ptr(),
            Message::Login(ref m) => m.as_raw_ptr(),
            Message::ConfCS(ref m) => m.as_raw_ptr(),
            Message::Stats(ref m) => m.as_raw_ptr(),
            Message::EnvTransfer(ref m) => m.as_raw_ptr(),
            Message::InternalStatus(ref m) => m.as_raw_ptr(),
            Message::MonitorLogin(ref m) => m.as_raw_ptr(),
            Message::MonitorGetCS(ref m) => m.as_raw_ptr(),
            Message::MonitorJobBegin(ref m) => m.as_raw_ptr(),
            Message::MonitorJobDone(ref m) => m.as_raw_ptr(),
            Message::MonitorLocalJobBegin(ref m) => m.as_raw_ptr(),
            Message::MonitorStats(ref m) => m.as_raw_ptr(),
            Message::Text(ref m) => m.as_raw_ptr(),
            Message::StatusText(ref m) => m.as_raw_ptr(),
            Message::VerifyEnv(ref m) => m.as_raw_ptr(),
            Message::VerifyEnvResult(ref m) => m.as_raw_ptr(),
            Message::BlacklistHostEnv(ref m) => m.as_raw_ptr(),
        }
    }
}


impl AsRef<Message> for Message {
    fn as_ref(&self) -> &Message {
        self
    }
}


#[derive(Clone)]
pub struct CompileJob {
    cj: Rc<ptr::CompileJob>,
}

impl AsPtr for CompileJob {
    type Output = sys::CompileJob;
    fn as_ptr(&self) -> *mut sys::CompileJob {
        self.cj.as_ptr()
    }
}

impl CompileJob
{
    fn from_raw_ptr(ptr: *mut sys::CompileJob) -> Self {
        assert_ne!(ptr, 0 as *mut sys::CompileJob);
        Self { cj: Rc::new(ptr::CompileJob(ptr)) }
    }

    pub fn new() -> Self {
        Self::from_raw_ptr(unsafe { sys::compile_job_new() })
    }

    accessors! {
        (u32
            job_id compile_job_id
            set_job_id compile_job_set_id)
        (Language
            language compile_job_language
            set_language compile_job_set_language)
        (String
            compiler_name compile_job_compiler_name
            set_compiler_name compile_job_set_compiler_name)
        (String
            environment_version compile_job_environment_version
            set_environment_version compile_job_set_environment_version)
        (String
            input_file compile_job_input_file
            set_input_file compile_job_set_input_file)
        (String
            output_file compile_job_output_file
            set_output_file compile_job_set_output_file)
        (String
            target_platform compile_job_target_platform
            set_target_platform compile_job_set_target_platform)
    }
}
