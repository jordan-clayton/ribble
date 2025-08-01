use crate::utils::errors::RibbleError;
use crash_handler::{make_crash_event, CrashContext, CrashEventResult, CrashHandler};

// NOTE: this could be a completely generic function -> if that's required, return a Result<CrashHandler> instead
// and coerce it at the call site.
pub(crate) fn set_up_desktop_crash_handler() -> Result<CrashHandler, RibbleError> {
    let handler = unsafe {
        CrashHandler::attach(make_crash_event(crash_popup))?
    };
    Ok(handler)
}


// TODO: consider lower-case hexadecimal for all, or treat the pointers as usize and Uppercase

// This is a "handled"-only sort of deal, by which I mean
// an OS pop-up will pop up if the program isn't already terminated
// The goal here is to provide a best-effort last notification to the user
// to give them more information in case there's a weird (likely GPU) exception.
fn crash_popup(crash_context: &CrashContext) -> CrashEventResult {
    // TODO: consider using cfg_if
    #[cfg(any(target_os = "linux", target_os = "android"))] {
        use exception_constants::*;
        let sig_info = crash_context.siginfo;

        // "Kind"
        let sig_no = sig_info.ssi_signo;
        // Error numbers are unused in linux, skip.

        // Pointer to the address that caused the signal.
        let addr = sig_info.ssi_addr;

        // "Human-Readable" Signal
        let error_description = match sig_no {
            SIGABRT => format!("Signal: SIGABRT. Abnormal process termination (from address: {addr:#04X})"),
            SIGBUS => format!("Signal: SIGBUS. Bus error (bad memory access at: {addr:#04X})"),
            SIGFPE => format!("Signal: SIGFPE. Arithmetic error. (fault address: {addr:#04X})"),
            SIGILL => format!("Signal: SIGILL. Illegal instruction. (fault address: {addr:#04X}"),
            // Ignore SIGTRAP -> breakpoints will set this off, and it should just return
            // There's nothing to alert.
            SIGTRAP => return CrashEventResult::Handled(true),
            SIGSEGV => format!("Signal: SIGSEGV. Address not mapped to object (fault address: {addr:#04X})"),

            // Consider all other signals "handled."
            // The main concern here is (GPU) segfaults.
            _ => return CrashEventResult::Handled(true),
        };

        // Show a quick and dirty OS dialog warning about the exception.
        rfd::MessageDialog::new()
            .set_title("Unhandled Exception!")
            .set_description(error_description)
            .set_buttons(rfd::MessageButtons::Ok)
            .set_level(rfd::MessageLevel::Error)
            .show();
    }

    #[cfg(target_os = "macos")]{
        use exception_constants::*;
        let exception_info = crash_context.exception.expect("There should be an exception if this handler is being called.");

        let kind = exception_info.kind;
        let exception_code = exception_info.code;
        let pid = crash_context.task;

        // For EXC_BAD_ACCESS, the subcode is the address.
        let mut exception_subcode = exception_info.subcode;

        let error_string = match kind {
            BAD_ACCESS => "BAD_ACCESS",
            BAD_INSTRUCTION => "BAD_INSTRUCTION",
            // This is essentially FPE/ZeroDiv error.
            ARITHMETIC => "ARITHMETIC",
            EMULATION => "EMULATION",
            SOFTWARE => "SOFTWARE",
            // Consider breakpoints "handled" by default
            TRAP => return CrashEventResult::Handled(true),
            GUARD => "GUARD",
            CORPSE_NOTIFY => "CORPSE_NOTIFY",
            CRASH => "ABNORMAL_PROCESS_EXIT",
            RESOURCE => "RESOURCE_CONSUMPTION_LIMIT_EXCEEDED",
            _ => return CrashEventResult::Handled(true),
        };

        // NOTE: the address is contained within the subcode for BadAccess (only?).
        // The next best thing would be to use the process ID, I suppose.
        let error_description = match exception_subcode {
            Some(subcode) => {
                format!("{error_string} in process: {pid}\n\
            Code: {exception_code:#04X}\n Subcode: {subcode:#04X}")
            }
            None => {
                format!("{error_string} in process:{pid}\n\
            Code: {exception_code:#04X}")
            }
        };

        // Show a quick and dirty OS dialog warning about the exception.
        rfd::MessageDialog::new()
            .set_title("Unhandled Exception!")
            .set_description(error_description)
            .set_buttons(rfd::MessageButtons::Ok)
            .set_level(rfd::MessageLevel::Error)
            .show();
    }

    #[cfg(target_os = "windows")]
    {
        use exception_constants::*;
        let exception_code = crash_context.exception_code;
        let error_string = match exception_code {
            ABORT => "ABORT",
            FPE => "FPE",
            ILLEGAL => "ILLEGAL",
            INVALID_PARAMETER => "INVALID_PARAMETER",
            PURE_CALL => "PURE_CALL",
            SEGFAULT => "SEGFAULT",
            STACK_OVERFLOW => "STACK_OVERFLOW",
            // Consider traps/breakpoints handled
            TRAP => return CrashEventResult::Handled(true),
            HEAP_CORRUPTION => "HEAP_CORRUPTION",
            _ => return CrashEventResult::Handled(true)
        };

        let exception_ptr = crash_context.exception_pointers;
        let address = unsafe {
            let record = (*exception_ptr).ExceptionRecord;
            (*record).ExceptionAddress
        };

        let error_description = format!("{error_string} at {address:#04X}", address);
        rfd::MessageDialog::new()
            .set_title("Unhandled Exception!")
            .set_description(error_description)
            .set_buttons(rfd::MessageButtons::Ok)
            .set_level(rfd::MessageLevel::Error)
            .show();
    }

    CrashEventResult::Handled(true)
}

// NOTE: if inline constants get stabilized in match expressions,
// consider moving the implementation back up into the appropriate config block.
// Otherwise, these are probably sufficient.
#[cfg(any(target_os = "linux", target_os = "android"))]
mod exception_constants {
    use crash_handler::Signal;
    pub const SIGABRT: u32 = Signal::Abort as u32;
    pub const SIGBUS: u32 = Signal::Bus as u32;
    pub const SIGFPE: u32 = Signal::Fpe as u32;
    pub const SIGILL: u32 = Signal::Illegal as u32;
    pub const SIGTRAP: u32 = Signal::Trap as u32;
    pub const SIGSEGV: u32 = Signal::Segv as u32;
}

#[cfg(target_os = "macos")]
mod exception_constants {
    use crash_handler::ExceptionType;

    // (SIGSEGV/SIGBUS) -> subcode is the bad memory address
    pub const BAD_ACCESS: u32 = ExceptionType::BadAccess as u32;
    // (SIGILL)
    pub const BAD_INSTRUCTION: u32 = ExceptionType::BadInstruction as u32;

    // (SIGFPE)
    pub const ARITHMETIC: u32 = ExceptionType::Arithmetic as u32;
    pub const EMULATION: u32 = ExceptionType::Emulation as u32;

    // (SIGABRT)
    pub const SOFTWARE: u32 = ExceptionType::Software as u32;
    // (SIGTRAP -> skip this)
    pub const TRAP: u32 = ExceptionType::Breakpoint as u32;
    // EXC_GUARD -> file-descriptor integrity problem, violated resource protection
    pub const GUARD: u32 = ExceptionType::Guard as u32;
    pub const CORPSE_NOTIFY: u32 = ExceptionType::CorpseNotify as u32;
    pub const CRASH: u32 = ExceptionType::Crash as u32;
    pub const RESOURCE: u32 = ExceptionType::Resource as u32;
}
#[cfg(target_os = "windows")]
mod exception_constants {
    use crash_handler::ExceptionCode;
    pub const ABORT: i32 = ExceptionCode::Abort as i32;
    pub const FPE: i32 = ExceptionCode::Fpe as i32;
    pub const ILLEGAL: i32 = ExceptionCode::Illegal as i32;
    pub const INVALID_PARAMETER: i32 = ExceptionCode::InvalidParameter as i32;
    pub const PURE_CALL: i32 = ExceptionCode::Purecall as i32;
    pub const SEGFAULT: i32 = ExceptionCode::Segv as i32;
    pub const STACK_OVERFLOW: i32 = ExceptionCode::StackOverflow as i32;
    pub const TRAP: i32 = ExceptionCode::Trap as i32;
    pub const HEAP_CORRUPTION: i32 = ExceptionCode::HeapCorruption as i32;
}