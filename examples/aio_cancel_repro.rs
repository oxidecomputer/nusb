//! Measure what happens when a bulk transfer is cancelled near its completion.
//!
//! Cancellation is asynchronous on every platform nusb supports: the request to
//! cancel does not wait, and the transfer completes later through the normal
//! path. This program drives that window deliberately. It submits a bulk IN
//! transfer, waits a chosen delay, cancels, and records both what came back and
//! how long the completion took to arrive, sweeping the delay so the cancel
//! lands at varying distances from the transfer's own completion.
//!
//! It reports three things beyond the transfer outcome, each of which is a
//! failure mode a cancellation path can have:
//!
//! * how long a completion takes to arrive after its cancel, as a distribution
//!   rather than a mean, because a bimodal tail is invisible in an average;
//! * how long the endpoint stays unopenable after it is dropped, which is
//!   visible to any program that closes and reopens a device;
//! * whether the process survived at all, since a fault in a completion
//!   callback aborts rather than unwinding.
//!
//! Each trial runs in a child process so that an abort costs one trial instead
//! of the run. The parent aggregates and prints; `--trial` is the child.
//!
//! Usage. Leaving transfers outstanding at endpoint drop is what provokes a
//! cancel against a transfer that has already completed:
//!
//!     aio_cancel_repro --device 38c6:0001 --interface 4 --ep-in 0x85 \
//!         --ep-out 0x05 --post-open 0001 --abandon 2 --trials 20
//!
//! Submitting a batch and draining it measures whether completions come back
//! in order, and how long that takes:
//!
//!     aio_cancel_repro --device 38c6:0001 --interface 4 --ep-in 0x85 \
//!         --ep-out 0x05 --order-check 32 --iterations 0 --timeout-ms 2000
//!
//! `--compare before=./a after=./b` alternates two builds trial by trial,
//! which is how two builds should be compared: the failure rate on a host
//! moves between sessions by more than the difference between builds.
//! `--format json` archives a run, and `--report FILE...` merges archived
//! runs, including runs from different hosts.
//!
//! The same binary runs on every platform nusb supports, so another platform
//! can be used as a control.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use clap::Parser;
use nusb::descriptors::TransferType;
use nusb::transfer::{Bulk, Direction, In, Out};
use nusb::{DeviceInfo, MaybeFuture};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- configuration

#[derive(Parser, Debug)]
#[command(
    about = "Cancel bulk transfers near completion and measure what happens",
    long_about = None,
)]
struct Args {
    /// Read the run from a JSON file: the devices, endpoints, delays, and
    /// batch sizes, so a run is reproducible from one artifact. The flags
    /// controlling how it is driven and printed still apply, namely --trials,
    /// --format, --build, --compare, and --report.
    #[arg(long, value_name = "FILE")]
    config: Option<String>,

    /// Device to drive, as `vid:pid` or `vid:pid:serial`, in hex. Repeatable.
    #[arg(long = "device", value_name = "VID:PID[:SERIAL]")]
    devices: Vec<String>,

    /// Interface to claim.
    #[arg(long, default_value_t = 0)]
    interface: u8,

    /// Bulk IN endpoint address, in hex, for example 0x81.
    #[arg(long, value_name = "ADDR", value_parser = parse_hex_u8)]
    ep_in: Option<u8>,

    /// Bulk OUT endpoint address. Needed only with --post-open.
    #[arg(long, value_name = "ADDR", value_parser = parse_hex_u8)]
    ep_out: Option<u8>,

    /// Bytes to write to the OUT endpoint before each read, as hex, for a
    /// device that answers only when asked. `0001` is a CMSIS-DAP DAP_Info
    /// request. Without this a read has nothing to race against, so a cancel
    /// can only catch a transfer still in flight, never one completing.
    #[arg(long, value_name = "HEX")]
    post_open: Option<String>,

    /// Cancellation trials per delay, per device. Zero skips them, leaving
    /// only --order-check, which is how a run measures the batch drain alone.
    #[arg(long, default_value_t = 20)]
    iterations: u32,

    /// Delay between submitting and cancelling, in microseconds.
    #[arg(long, default_value_t = 50_000)]
    cancel_delay_us: u64,

    /// Sweep the cancel delay instead, as `min,max,step` in microseconds.
    #[arg(long, value_name = "MIN,MAX,STEP")]
    cancel_delay_sweep: Option<String>,

    /// Transfers submitted before the cancel. Depth above one exercises a
    /// queue being cancelled as a whole.
    #[arg(long, default_value_t = 1)]
    queue_depth: u32,

    /// Transfers to abandon per device: submitted, cancelled, and left
    /// outstanding when the endpoint is dropped. This is what a program does
    /// when it gives up on a device and moves to the next one, and it is how a
    /// backlog of late completions builds up.
    #[arg(long, default_value_t = 0)]
    abandon: u32,

    /// Repeat the whole device list this many times, so completions abandoned
    /// on an earlier pass are still outstanding during a later one.
    #[arg(long, default_value_t = 1)]
    rounds: u32,

    /// Submit this many writes at once and check that they are returned in
    /// submission order and without stalling. Needs --ep-out. Zero skips it.
    /// Each write carries its identity in one payload byte, so the batch can
    /// be at most 254.
    #[arg(long, default_value_t = 0)]
    order_check: u32,

    /// Read buffer size in bytes.
    #[arg(long, default_value_t = 512)]
    transfer_len: usize,

    /// How long to wait for a completion before giving up on it.
    #[arg(long, default_value_t = 30_000)]
    timeout_ms: u64,

    /// Trials to run, each in its own process.
    #[arg(long, default_value_t = 10)]
    trials: u32,

    /// How long to keep retrying the endpoint reopen after a trial. Zero
    /// tries once and reports what happened, which measures the state
    /// immediately after the drop.
    #[arg(long, default_value_t = 30_000)]
    reopen_timeout_ms: u64,

    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Table)]
    format: Format,

    /// Label for this build, carried in the record stream so several builds
    /// can be compared from one file.
    #[arg(long, default_value = "default")]
    build: String,

    /// Compare several builds, as NAME=PATH, alternating them trial by trial.
    /// Each path is another build of this same example.
    #[arg(long, value_name = "NAME=PATH", num_args = 1..)]
    compare: Vec<String>,

    /// Read record streams back and print one table across their builds, for
    /// comparing runs from different hosts. Everything else is ignored.
    #[arg(long, value_name = "FILE", num_args = 1..)]
    report: Vec<String>,

    /// List the interfaces this example can drive on each named device, then
    /// stop. Needs --device or --config, so that it opens only what it is
    /// told to. `list` is the example that shows what is attached.
    #[arg(long)]
    list: bool,

    /// Run a single trial and emit its records. Used for the child process.
    #[arg(long, hide = true)]
    trial: bool,

    /// Configuration as JSON on the command line. Used for the child process.
    #[arg(long, hide = true, value_name = "JSON")]
    config_json: Option<String>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, clap::ValueEnum)]
enum Format {
    /// One JSON object per line, then a summary object.
    Json,
    /// A summary table and latency histograms.
    Table,
    /// One CSV row per transfer, for plotting elsewhere.
    Csv,
}

/// The resolved run, everything the child needs and nothing it does not.
///
/// This is also the shape `--config` reads, which is what makes a run
/// reproducible from one artifact. Note that the flags take endpoint addresses
/// in hex and this takes numbers, so `--ep-in 0x85` is `"ep_in": 133`.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Config {
    /// Devices to drive, each `vid:pid` or `vid:pid:serial` in hex.
    devices: Vec<String>,
    /// Interface to claim on each device.
    interface: u8,
    /// Bulk IN endpoint address, as a number.
    ep_in: u8,
    /// Bulk OUT endpoint address. Needed by `post_open` and `order_check`.
    ep_out: Option<u8>,
    /// Bytes to write before each read, as hex, for a device that answers
    /// only when asked. Without it a read has nothing to race against, and a
    /// cancel can only ever catch a transfer still in flight.
    post_open: Option<String>,
    /// Cancellation trials per delay, per device. Zero skips them, which is
    /// how a run measures only the batch drain.
    iterations: u32,
    /// Delays between submitting and cancelling, in microseconds. One entry
    /// per delay to try; `--cancel-delay-sweep` fills this from a range.
    delays_us: Vec<u64>,
    /// Transfers submitted before each cancel.
    queue_depth: u32,
    /// Transfers left outstanding when the endpoint is dropped, which is what
    /// a program does when it gives up on a device.
    abandon: u32,
    /// Passes over the device list. Above one, completions abandoned on an
    /// earlier pass are still outstanding during a later one.
    rounds: u32,
    /// Writes submitted at once for the ordering check, at most 254, which
    /// is what one payload byte of identity allows. Zero skips it.
    order_check: u32,
    /// Read buffer size in bytes.
    transfer_len: usize,
    /// How long to wait for one completion before giving up on it.
    timeout_ms: u64,
    /// How long to keep retrying the endpoint reopen after a trial. Zero
    /// tries once and reports what happened.
    reopen_timeout_ms: u64,
}

/// Read a run from a file, without judging whether it would measure anything.
fn read_config(path: &str) -> Result<Config, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("cannot parse {path}: {e}"))
}

/// Just the devices a file names, so an incomplete file still lists.
#[derive(Deserialize)]
struct ConfigDevices {
    devices: Vec<String>,
}

fn read_config_devices(path: &str) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    serde_json::from_str::<ConfigDevices>(&text)
        .map(|c| c.devices)
        .map_err(|e| format!("cannot parse {path}: {e}"))
}

impl Config {
    fn from_args(args: &Args) -> Result<Config, String> {
        if let Some(path) = &args.config {
            let config = read_config(path)?;
            config.validate()?;
            return Ok(config);
        }

        let ep_in = args
            .ep_in
            .ok_or("no IN endpoint; pass --ep-in, for example --ep-in 0x81")?;

        let delays_us = match &args.cancel_delay_sweep {
            Some(spec) => parse_sweep(spec)?,
            None => vec![args.cancel_delay_us],
        };

        let config = Config {
            devices: args.devices.clone(),
            interface: args.interface,
            ep_in,
            ep_out: args.ep_out,
            post_open: args.post_open.clone(),
            iterations: args.iterations,
            delays_us,
            queue_depth: args.queue_depth,
            abandon: args.abandon,
            rounds: args.rounds,
            order_check: args.order_check,
            transfer_len: args.transfer_len,
            timeout_ms: args.timeout_ms,
            reopen_timeout_ms: args.reopen_timeout_ms,
        };
        config.validate()?;
        Ok(config)
    }

    /// Reject a run that would measure nothing, or measure it wrongly.
    ///
    /// Flags and files both come through here. Each of these would otherwise
    /// produce a plausible-looking result rather than an error.
    fn validate(&self) -> Result<(), String> {
        if self.devices.is_empty() {
            return Err("no devices; pass --device VID:PID[:SERIAL]".into());
        }
        if self.post_open.is_some() && self.ep_out.is_none() {
            return Err("post_open needs an out endpoint".into());
        }
        if self.order_check > MAX_ORDER_CHECK {
            return Err(format!(
                "order_check is at most {MAX_ORDER_CHECK}, one payload byte of identity"
            ));
        }
        if self.queue_depth == 0 {
            return Err("queue_depth of 0 submits nothing".into());
        }
        if self.rounds == 0 {
            return Err("rounds of 0 runs nothing".into());
        }
        if self.timeout_ms == 0 {
            return Err("timeout_ms of 0 gives every wait nothing to wait for".into());
        }
        if self.delays_us.is_empty() {
            return Err("no cancel delays".into());
        }
        if self.transfer_len == 0 && (self.iterations > 0 || self.abandon > 0) {
            return Err("transfer_len of 0 completes before a cancel can land".into());
        }
        if self.iterations == 0 && self.order_check == 0 && self.abandon == 0 {
            return Err("nothing to measure; set iterations, order_check, or abandon".into());
        }
        Ok(())
    }
}

/// The largest batch the ordering check can identify.
///
/// Each write carries its identity in one payload byte, so a larger batch
/// would reuse tags and the comparison against submission order would pass on
/// transfers it could no longer tell apart.
const MAX_ORDER_CHECK: u32 = 254;

fn parse_hex_u8(s: &str) -> Result<u8, String> {
    let t = s.trim_start_matches("0x").trim_start_matches("0X");
    u8::from_str_radix(t, 16).map_err(|e| format!("{s} is not a hex byte: {e}"))
}

fn parse_sweep(spec: &str) -> Result<Vec<u64>, String> {
    let parts: Vec<&str> = spec.split(',').collect();
    if parts.len() != 3 {
        return Err(format!("{spec} is not min,max,step"));
    }
    let n = |s: &str| s.trim().parse::<u64>().map_err(|e| format!("{s}: {e}"));
    let (min, max, step) = (n(parts[0])?, n(parts[1])?, n(parts[2])?);
    if step == 0 || max < min {
        return Err(format!("{spec} does not describe a range"));
    }
    Ok((min..=max).step_by(step as usize).collect())
}

/// A device named as `vid:pid` or `vid:pid:serial`, all hex.
#[derive(Clone, Debug)]
struct DeviceSpec {
    vid: u16,
    pid: u16,
    serial: Option<String>,
    /// The text this was parsed from, so a report names what was asked for.
    as_given: String,
}

impl DeviceSpec {
    fn parse(s: &str) -> Result<DeviceSpec, String> {
        let mut it = s.splitn(3, ':');
        let vid = it.next().ok_or_else(|| format!("{s}: no vendor id"))?;
        let pid = it.next().ok_or_else(|| format!("{s}: no product id"))?;
        let hex = |v: &str| {
            u16::from_str_radix(v.trim_start_matches("0x"), 16)
                .map_err(|e| format!("{s}: {v} is not hex: {e}"))
        };
        Ok(DeviceSpec {
            vid: hex(vid)?,
            pid: hex(pid)?,
            serial: it.next().map(str::to_string).filter(|v| !v.is_empty()),
            as_given: s.to_string(),
        })
    }

    fn matches(&self, d: &DeviceInfo) -> bool {
        d.vendor_id() == self.vid
            && d.product_id() == self.pid
            && match &self.serial {
                Some(want) => d.serial_number() == Some(want.as_str()),
                None => true,
            }
    }
}

// ---------------------------------------------------------------------- records

/// One line of the stream. Everything the child observes is one of these, so
/// the parent can aggregate without knowing how the trial was structured.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Record {
    /// A transfer that was submitted, cancelled, and waited for.
    Transfer {
        device: String,
        delay_us: u64,
        iteration: u32,
        /// Time from the cancel to this completion arriving. A cancel covers
        /// the whole queue, so above depth one these accumulate.
        latency_us: u64,
        /// `cancelled`, `ok`, a transfer error name, or `timeout`.
        outcome: String,
        actual_len: usize,
    },
    /// Reopening the endpoint after the trial dropped it.
    Reopen {
        device: String,
        /// Time until the reopen succeeded.
        elapsed_us: u64,
        attempts: u32,
        /// The error seen on the first failed attempt, if any.
        first_error: Option<String>,
        succeeded: bool,
    },
    /// A batch of writes submitted together and drained. `order` holds the
    /// tag of each completion as it came back, so it is `0, 1, 2, ...` when
    /// the queue returns in submission order.
    Order {
        device: String,
        submitted: u32,
        order: Vec<u32>,
        drain_us: u64,
        /// The timeout the drain ran under. A drain that is pinned to this
        /// value is waiting rather than working, which is the distinction the
        /// number is there to make.
        timeout_ms: u64,
        stalled: bool,
    },
    /// Names the build whose records follow, up to the next such marker.
    Build { name: String, platform: String },
    /// Transfers left outstanding when the endpoint was dropped.
    Abandoned {
        device: String,
        delay_us: u64,
        transfers: usize,
    },
    /// Something went wrong that is not a transfer outcome.
    Problem { device: String, message: String },
    /// A child that did not exit cleanly.
    TrialFailed {
        trial: u32,
        status: String,
        message: Option<String>,
    },
}

fn emit(r: &Record) {
    // The record stream is the program's data output. Diagnostics go to stderr
    // so this stays machine readable on its own.
    println!("{}", serde_json::to_string(r).expect("record serializes"));
}

// ------------------------------------------------------------------- the trial

fn main() {
    env_logger::init();
    let args = Args::parse();

    if !args.report.is_empty() {
        report(&args.report);
        return;
    }

    if args.list {
        // A config file names devices too, and is the one place a reader most
        // wants to check what a published run refers to. Listing takes only
        // the device list from it, without validating the rest: a run that was
        // rejected is a reason to look at its devices, not a reason to refuse.
        let devices = match &args.config {
            Some(path) => match read_config_devices(path) {
                Ok(devices) => devices,
                Err(e) => fail(&e),
            },
            None => args.devices.clone(),
        };
        if devices.is_empty() {
            fail("--list needs --device or --config; nothing to look at");
        }
        list_interfaces(&devices);
        return;
    }

    let config = match &args.config_json {
        Some(json) => match serde_json::from_str::<Config>(json)
            .map_err(|e| format!("cannot parse --config-json: {e}"))
            .and_then(|c| c.validate().map(|()| c))
        {
            Ok(c) => c,
            Err(e) => fail(&e),
        },
        None => match Config::from_args(&args) {
            Ok(c) => c,
            Err(e) => fail(&e),
        },
    };

    if args.trial {
        run_trial(&config);
    } else {
        supervise(&args, &config);
    }
}

/// Exit status for a configuration the program will not run, kept distinct
/// from a trial that died so the supervisor can tell them apart.
const CONFIG_ERROR: i32 = 2;

fn fail(message: &str) -> ! {
    eprintln!("aio_cancel_repro: {message}");
    std::process::exit(CONFIG_ERROR);
}

/// Run every device, every delay, every iteration, emitting records as it goes.
fn run_trial(config: &Config) {
    for round in 0..config.rounds {
        for spec_str in &config.devices {
            let spec = match DeviceSpec::parse(spec_str) {
                Ok(s) => s,
                Err(e) => {
                    emit(&Record::Problem {
                        device: spec_str.clone(),
                        message: e,
                    });
                    continue;
                }
            };

            for &delay_us in &config.delays_us {
                if let Err(e) = drive_device(config, &spec, spec_str, delay_us) {
                    emit(&Record::Problem {
                        device: spec_str.clone(),
                        message: e,
                    });
                }
            }

            // Only worth measuring on the last round; earlier rounds reopen
            // immediately anyway as part of the next pass.
            if round + 1 == config.rounds {
                measure_reopen(config, &spec, spec_str);
            }
        }
    }
}

/// Print the interfaces a device offers, so a caller can choose one.
///
/// Interfaces without a bulk endpoint are omitted unless they say they speak
/// CMSIS-DAP, which is how a v1 probe shows that this example cannot drive
/// it.
fn list_interfaces(devices: &[String]) {
    let specs: Vec<DeviceSpec> = devices
        .iter()
        .filter_map(|d| match DeviceSpec::parse(d) {
            Ok(spec) => Some(spec),
            Err(e) => {
                eprintln!("{e}");
                None
            }
        })
        .collect();

    let all = match nusb::list_devices().wait() {
        Ok(all) => all,
        Err(e) => fail(&format!("list_devices: {e}")),
    };

    let all: Vec<DeviceInfo> = all.collect();
    let mut reported: Vec<&str> = Vec::new();
    for spec in &specs {
        if all.iter().any(|info| spec.matches(info)) {
            continue;
        }
        if reported.contains(&spec.as_given.as_str()) {
            continue;
        }
        reported.push(&spec.as_given);
        println!("{} not attached", spec.as_given);
    }

    for info in all {
        if !specs.iter().any(|s| s.matches(&info)) {
            continue;
        }

        println!(
            "{:04x}:{:04x}{} {}",
            info.vendor_id(),
            info.product_id(),
            info.serial_number()
                .map(|s| format!(":{s}"))
                .unwrap_or_default(),
            info.product_string().unwrap_or(""),
        );

        let device = match info.open().wait() {
            Ok(device) => device,
            Err(e) => {
                println!("  cannot open: {e}");
                continue;
            }
        };
        let configuration = match device.active_configuration() {
            Ok(configuration) => configuration,
            Err(e) => {
                println!("  cannot read the active configuration: {e}");
                continue;
            }
        };

        let mut listed = 0;
        let mut drivable = false;
        for alt in configuration.interface_alt_settings() {
            let bulk: Vec<String> = alt
                .endpoints()
                .filter(|e| e.transfer_type() == TransferType::Bulk)
                .map(|e| {
                    let dir = match e.direction() {
                        Direction::In => "in",
                        Direction::Out => "out",
                    };
                    format!("{dir} {:#04x}", e.address())
                })
                .collect();

            // The interface string is how a debug probe declares what its
            // interface speaks, and it is what probe-rs matches on.
            let name = info
                .interfaces()
                .find(|i| i.interface_number() == alt.interface_number())
                .and_then(|i| i.interface_string())
                .unwrap_or("");
            let note = cmsis_dap_note(name, alt.class(), alt.subclass());

            // An interface with no bulk endpoint is listed only when it says
            // it speaks CMSIS-DAP, which is how a v1 probe explains itself:
            // that form runs over HID interrupt transfers, so it appears here
            // with nothing this example can drive.
            if bulk.is_empty() && note.is_empty() {
                continue;
            }
            listed += 1;
            drivable |= !bulk.is_empty() && note.contains("v2");

            let endpoints = if bulk.is_empty() {
                "no bulk endpoints".to_string()
            } else {
                bulk.join("  ")
            };

            println!(
                "  interface {:<3} class {:02x}.{:02x}.{:02x} {:<16} {:<24} {}{}",
                alt.interface_number(),
                alt.class(),
                alt.subclass(),
                alt.protocol(),
                class_name(alt.class()),
                endpoints,
                name,
                note,
            );
        }
        if listed == 0 {
            println!("  nothing this example can drive");
        } else if !drivable {
            // A probe offering only the HID form of CMSIS-DAP still lists a
            // serial port, whose bulk endpoints look usable from here. Point
            // at the question rather than answering it, since what else a
            // device puts on a bulk interface is its own business.
            println!("  no CMSIS-DAP v2 interface; check what the bulk endpoints above are");
        }
    }
}

/// Say whether probe-rs would treat this interface as CMSIS-DAP.
///
/// The convention is from the CMSIS-DAP specification: the interface string
/// carries "CMSIS-DAP", with class and subclass 0xff and 0 for the bulk form
/// or the HID class for the interrupt form. The class alone is not enough.
fn cmsis_dap_note(name: &str, class: u8, subclass: u8) -> &'static str {
    if !(name.contains("CMSIS-DAP") || name.contains("CMSIS_DAP")) {
        return "";
    }
    match (class, subclass) {
        (0xff, 0x00) => "  <- CMSIS-DAP v2",
        (0x03, _) => "  <- CMSIS-DAP v1",
        _ => "",
    }
}

/// Name the interface classes a reader is likely to meet here.
fn class_name(class: u8) -> &'static str {
    match class {
        0x03 => "HID",
        0x06 => "image",
        0x07 => "printer",
        0x08 => "mass storage",
        0x0a => "CDC data",
        0xff => "vendor specific",
        _ => "unrecognised",
    }
}

/// Submit, wait, cancel, and wait for the completion, `iterations` times.
///
/// The endpoint is dropped when this returns, which is what `measure_reopen`
/// then observes.
fn drive_device(
    config: &Config,
    spec: &DeviceSpec,
    name: &str,
    delay_us: u64,
) -> Result<(), String> {
    let info = find_device(spec)?;
    let device = info.open().wait().map_err(|e| format!("open: {e}"))?;
    let interface = device
        .claim_interface(config.interface)
        .wait()
        .map_err(|e| format!("claim_interface {}: {e}", config.interface))?;

    let mut ep_in = interface
        .endpoint::<Bulk, In>(config.ep_in)
        .map_err(|e| format!("endpoint {:#04x}: {e}", config.ep_in))?;
    // The OUT endpoint serves both the prompt and the ordering batch, so open
    // it when either wants it.
    let wants_out = config.post_open.is_some() || config.order_check > 0;
    let mut ep_out = match (config.ep_out, wants_out) {
        (Some(addr), true) => Some(
            interface
                .endpoint::<Bulk, Out>(addr)
                .map_err(|e| format!("endpoint {addr:#04x}: {e}"))?,
        ),
        _ => None,
    };
    let prompt = match &config.post_open {
        Some(hex) => Some(parse_hex_bytes(hex)?),
        None => None,
    };

    let timeout = Duration::from_millis(config.timeout_ms);

    for iteration in 0..config.iterations {
        // Some devices answer only when asked. Send the prompt first so the
        // read has something to race against.
        if let (Some(ep), Some(bytes)) = (ep_out.as_mut(), prompt.as_ref()) {
            ep.submit(bytes.clone().into());
            if ep.wait_next_complete(timeout).is_none() {
                emit(&Record::Problem {
                    device: name.to_string(),
                    message: "prompt write did not complete".into(),
                });
            }
        }

        for _ in 0..config.queue_depth {
            let buffer = ep_in.allocate(config.transfer_len);
            ep_in.submit(buffer);
        }

        std::thread::sleep(Duration::from_micros(delay_us));

        // The measurement starts here. Cancellation is asynchronous, so what
        // follows is how long the completion took to come back, not how long
        // the cancel took.
        let cancelled_at = Instant::now();
        ep_in.cancel_all();

        while ep_in.pending() > 0 {
            // A timeout leaves the transfer queued, so there is nothing to
            // wait for a second time. Record what is missing and move on.
            let Some(completion) = ep_in.wait_next_complete(timeout) else {
                emit(&Record::Problem {
                    device: name.to_string(),
                    message: format!(
                        "{} transfers had not completed {}ms after cancel",
                        ep_in.pending(),
                        config.timeout_ms
                    ),
                });
                break;
            };
            emit(&Record::Transfer {
                device: name.to_string(),
                delay_us,
                iteration,
                latency_us: cancelled_at.elapsed().as_micros() as u64,
                outcome: match completion.status {
                    Ok(()) => "ok".to_string(),
                    Err(e) => format!("{e:?}").to_lowercase(),
                },
                actual_len: completion.actual_len,
            });
        }
    }

    // Submit a batch and check it comes back whole, in order, without
    // stalling. nusb publishes an endpoint's completions strictly from the
    // head of its queue, so a backend that completes them out of order shows
    // up here as a drain that takes far longer than the transfers did, or one
    // that gives up with transfers still outstanding.
    if config.order_check > 0 {
        match ep_out.as_mut() {
            Some(ep) => measure_order(config, ep, name, timeout),
            None => emit(&Record::Problem {
                device: name.to_string(),
                message: "--order-check needs --ep-out".into(),
            }),
        }
    }

    // Leave transfers outstanding and walk away, which is what a program does
    // when it gives up on a device. The endpoint drops here with these still
    // in flight, so their completions arrive with nobody waiting.
    if config.abandon > 0 {
        if let (Some(ep), Some(bytes)) = (ep_out.as_mut(), prompt.as_ref()) {
            ep.submit(bytes.clone().into());
        }
        for _ in 0..config.abandon {
            let buffer = ep_in.allocate(config.transfer_len);
            ep_in.submit(buffer);
        }
        std::thread::sleep(Duration::from_micros(delay_us));
        ep_in.cancel_all();
        emit(&Record::Abandoned {
            device: name.to_string(),
            delay_us,
            // The prompt write is left outstanding alongside the reads.
            transfers: ep_in.pending() + ep_out.as_ref().map_or(0, |ep| ep.pending()),
        });
    }

    Ok(())
}

/// Submit a batch of writes at once and check how the queue drains.
///
/// Each write carries a tag in its payload and comes back with its buffer, so
/// the order the batch returns in is directly observable.
fn measure_order(
    config: &Config,
    ep: &mut nusb::Endpoint<Bulk, Out>,
    name: &str,
    timeout: Duration,
) {
    for tag in 0..config.order_check {
        // Byte 0 is the command, byte 1 both varies the request and tags the
        // buffer so its completion can be identified.
        let payload = vec![0x00u8, tag as u8 + 1];
        ep.submit(payload.into());
    }

    let started = Instant::now();
    let mut order = Vec::with_capacity(config.order_check as usize);
    let mut stalled = false;

    while ep.pending() > 0 {
        let Some(completion) = ep.wait_next_complete(timeout) else {
            stalled = true;
            break;
        };
        // Recover the tag from the buffer that came back. A buffer too short
        // to carry one is recorded as an unidentifiable completion rather
        // than silently counted as in order.
        match completion.buffer.get(1) {
            Some(&tag) => order.push(u32::from(tag).saturating_sub(1)),
            None => order.push(u32::MAX),
        }
    }

    emit(&Record::Order {
        device: name.to_string(),
        submitted: config.order_check,
        order,
        drain_us: started.elapsed().as_micros() as u64,
        timeout_ms: config.timeout_ms,
        stalled,
    });
}

/// Reopen the endpoint that the trial just dropped, timing how long it takes.
///
/// The timing covers the open, the claim, and the endpoint, which are the
/// operations that can report the device busy.
fn measure_reopen(config: &Config, spec: &DeviceSpec, name: &str) {
    // Enumerate before the clock starts. Walking the bus costs milliseconds
    // and differs by an order of magnitude between platforms, which would
    // swamp the window being measured and make two platforms incomparable.
    let info = match find_device(spec) {
        Ok(info) => info,
        Err(e) => {
            emit(&Record::Problem {
                device: name.to_string(),
                message: e,
            });
            return;
        }
    };

    let started = Instant::now();
    let deadline = Duration::from_millis(config.reopen_timeout_ms);
    let mut attempts = 0u32;
    let mut first_error = None;

    loop {
        attempts += 1;
        match try_open_endpoint(config, &info) {
            Ok(()) => {
                emit(&Record::Reopen {
                    device: name.to_string(),
                    elapsed_us: started.elapsed().as_micros() as u64,
                    attempts,
                    first_error,
                    succeeded: true,
                });
                return;
            }
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
                if started.elapsed() >= deadline {
                    emit(&Record::Reopen {
                        device: name.to_string(),
                        elapsed_us: started.elapsed().as_micros() as u64,
                        attempts,
                        first_error,
                        succeeded: false,
                    });
                    return;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

fn try_open_endpoint(config: &Config, info: &DeviceInfo) -> Result<(), String> {
    let device = info.open().wait().map_err(|e| format!("open: {e}"))?;
    let interface = device
        .claim_interface(config.interface)
        .wait()
        .map_err(|e| format!("claim_interface: {e}"))?;
    interface
        .endpoint::<Bulk, In>(config.ep_in)
        .map(|_| ())
        .map_err(|e| format!("endpoint: {e}"))
}

fn find_device(spec: &DeviceSpec) -> Result<DeviceInfo, String> {
    nusb::list_devices()
        .wait()
        .map_err(|e| format!("list_devices: {e}"))?
        .find(|d| spec.matches(d))
        .ok_or_else(|| {
            format!(
                "no device {:04x}:{:04x}{}",
                spec.vid,
                spec.pid,
                spec.serial
                    .as_deref()
                    .map(|s| format!(":{s}"))
                    .unwrap_or_default()
            )
        })
}

fn parse_hex_bytes(s: &str) -> Result<Vec<u8>, String> {
    let clean: Vec<u8> = s
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect::<Vec<u8>>();
    if !clean.iter().all(u8::is_ascii_hexdigit) {
        return Err(format!("{s} is not hex"));
    }
    if clean.len() % 2 != 0 {
        return Err(format!("{s} has an odd number of hex digits"));
    }
    clean
        .chunks(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).expect("ascii hex digits");
            u8::from_str_radix(text, 16).map_err(|e| format!("{s}: {e}"))
        })
        .collect()
}

// -------------------------------------------------------------- the supervisor

/// Run trials as child processes and collect their records.
///
/// A fault in a completion callback aborts rather than unwinding, so a trial
/// that dies costs a trial. With `--compare`, each round runs one trial of
/// every build, because failure rates move between sessions by more than the
/// difference between two builds.
fn supervise(args: &Args, config: &Config) {
    let builds = match resolve_builds(args) {
        Ok(a) => a,
        Err(e) => fail(&e),
    };
    let config_json = serde_json::to_string(config).expect("config serializes");
    let mut collected: Vec<(String, Vec<Record>)> = builds
        .iter()
        .map(|(n, _)| (n.clone(), Vec::new()))
        .collect();

    for trial in 0..args.trials {
        for (i, (_, exe)) in builds.iter().enumerate() {
            collected[i]
                .1
                .extend(run_trial_process(exe, &config_json, trial));
        }
    }

    let platform = std::env::consts::OS.to_string();
    match args.format {
        Format::Json => {
            for (name, records) in &collected {
                emit(&Record::Build {
                    name: name.clone(),
                    platform: platform.clone(),
                });
                for r in records {
                    emit(r);
                }
                // Count this build's aborts from its own records. A total across
                // builds would report a build that never died as having died.
                println!(
                    "{}",
                    serde_json::to_string(&Summary::of(records, args.trials, aborts_in(records)))
                        .expect("summary serializes")
                );
            }
        }
        Format::Csv => {
            for (_, records) in &collected {
                print_csv(records);
            }
        }
        Format::Table if args.compare.is_empty() => {
            let records = &collected[0].1;
            print_table(records, args.trials, aborts_in(records), config)
        }
        Format::Table => print_builds(
            &collected
                .iter()
                .map(|(n, r)| (n.clone(), platform.clone(), r.clone()))
                .collect::<Vec<_>>(),
        ),
    }
}

/// The builds to run, as name and path. Without `--compare` that is this
/// binary alone, under whatever `--build` says.
fn resolve_builds(args: &Args) -> Result<Vec<(String, PathBuf)>, String> {
    if args.compare.is_empty() {
        let exe = std::env::current_exe().map_err(|e| format!("cannot find own path: {e}"))?;
        return Ok(vec![(args.build.clone(), exe)]);
    }
    args.compare
        .iter()
        .map(|spec| match spec.split_once('=') {
            Some((name, path)) if !name.is_empty() && !path.is_empty() => {
                Ok((name.to_string(), PathBuf::from(path)))
            }
            _ => Err(format!("{spec} is not NAME=PATH")),
        })
        .collect()
}

/// How many trials in this record stream died.
fn aborts_in(records: &[Record]) -> u32 {
    records
        .iter()
        .filter(|r| matches!(r, Record::TrialFailed { .. }))
        .count() as u32
}

/// Run one trial and return its records. A trial that died contributes a
/// `TrialFailed` record.
fn run_trial_process(exe: &Path, config_json: &str, trial: u32) -> Vec<Record> {
    let out = Command::new(exe)
        .arg("--trial")
        .arg("--config-json")
        .arg(config_json)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let out = match out {
        Ok(o) => o,
        Err(e) => fail(&format!("cannot run {}: {e}", exe.display())),
    };

    let mut records = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        match serde_json::from_str::<Record>(line) {
            Ok(r) => records.push(r),
            Err(e) => eprintln!("trial {trial}: unparsable record: {e}"),
        }
    }

    // A child that rejected its configuration exits with CONFIG_ERROR.
    // Counting it as an aborted trial would read as a reproduction of the
    // fault being measured, so stop and say what is wrong instead.
    if out.status.code() == Some(CONFIG_ERROR) {
        fail(String::from_utf8_lossy(&out.stderr).trim());
    }

    if out.status.success() {
        return records;
    }

    let stderr = String::from_utf8_lossy(&out.stderr);
    records.push(Record::TrialFailed {
        trial,
        status: format!("{}", out.status),
        message: stderr
            .lines()
            .find(|l| l.contains("panicked") || l.contains("Abort"))
            .map(str::to_string),
    });
    records
}

// ----------------------------------------------------------------- aggregation

#[derive(Serialize)]
struct Summary {
    trials: u32,
    trials_failed: u32,
    transfers: usize,
    outcomes: Vec<(String, usize)>,
    latency_us: Quantiles,
    reopen_us: Quantiles,
}

impl Summary {
    fn of(records: &[Record], trials: u32, failed: u32) -> Summary {
        let mut outcomes: Vec<(String, usize)> = Vec::new();
        let mut latency = Vec::new();
        let mut reopen = Vec::new();
        let mut transfers = 0;

        for r in records {
            match r {
                Record::Transfer {
                    latency_us,
                    outcome,
                    ..
                } => {
                    transfers += 1;
                    latency.push(*latency_us);
                    match outcomes.iter_mut().find(|(k, _)| k == outcome) {
                        Some((_, n)) => *n += 1,
                        None => outcomes.push((outcome.clone(), 1)),
                    }
                }
                Record::Reopen { elapsed_us, .. } => reopen.push(*elapsed_us),
                _ => {}
            }
        }
        outcomes.sort();

        Summary {
            trials,
            trials_failed: failed,
            transfers,
            outcomes,
            latency_us: Quantiles::of(&mut latency),
            reopen_us: Quantiles::of(&mut reopen),
        }
    }
}

#[derive(Serialize, Default)]
struct Quantiles {
    n: usize,
    min: u64,
    p50: u64,
    p90: u64,
    p99: u64,
    max: u64,
}

impl Quantiles {
    fn of(values: &mut [u64]) -> Quantiles {
        if values.is_empty() {
            return Quantiles::default();
        }
        values.sort_unstable();
        let at = |q: f64| values[((values.len() - 1) as f64 * q) as usize];
        Quantiles {
            n: values.len(),
            min: values[0],
            p50: at(0.50),
            p90: at(0.90),
            p99: at(0.99),
            max: values[values.len() - 1],
        }
    }
}

// ------------------------------------------------------------ cross-build report

/// Read record streams back and compare the builds that produced them.
///
/// Each stream names its build, so files from different hosts can be
/// concatenated in any order.
fn report(paths: &[String]) {
    let mut builds: Vec<(String, String, Vec<Record>)> = Vec::new();

    for path in paths {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => fail(&format!("cannot read {path}: {e}")),
        };
        let mut current: Option<usize> = None;
        for line in text.lines() {
            // A stream also carries summary objects, which have no tag and do
            // not parse as records. Skipping them costs nothing.
            let Ok(record) = serde_json::from_str::<Record>(line) else {
                continue;
            };
            if let Record::Build { name, platform } = &record {
                current = Some(match builds.iter().position(|(n, _, _)| n == name) {
                    Some(i) => i,
                    None => {
                        builds.push((name.clone(), platform.clone(), Vec::new()));
                        builds.len() - 1
                    }
                });
                continue;
            }
            match current {
                Some(i) => builds[i].2.push(record),
                None => fail(&format!("{path}: records before any build marker")),
            }
        }
    }

    if builds.is_empty() {
        fail("no records found");
    }
    print_builds(&builds);
}

/// One row per build, then a latency histogram for each. The histograms carry
/// the tail, which is what a caller waiting on a cancelled transfer meets.
fn print_builds(builds: &[(String, String, Vec<Record>)]) {
    println!(
        "{:<20} {:<9} {:>8} {:>7} {:>8} {:>8} {:>9} {:>9} {:>10}",
        "build",
        "platform",
        "aborts",
        "xfers",
        "p50 ms",
        "max ms",
        "reopen p50",
        "reopen max",
        "queue"
    );
    for (name, platform, records) in builds {
        let aborts = aborts_in(records);
        let mut latency: Vec<u64> = records
            .iter()
            .filter_map(|r| match r {
                Record::Transfer { latency_us, .. } => Some(*latency_us),
                _ => None,
            })
            .collect();
        let mut reopen: Vec<u64> = records
            .iter()
            .filter_map(|r| match r {
                Record::Reopen { elapsed_us, .. } => Some(*elapsed_us),
                _ => None,
            })
            .collect();
        let l = Quantiles::of(&mut latency);
        let o = Quantiles::of(&mut reopen);
        println!(
            "{:<20} {:<9} {:>8} {:>7} {:>8} {:>8} {:>9} {:>9} {:>10}",
            elide(name, 20),
            platform,
            aborts,
            l.n,
            or_absent(l.n, ms(l.p50)),
            or_absent(l.n, ms(l.max)),
            or_absent(o.n, ms(o.p50)),
            or_absent(o.n, ms(o.max)),
            describe_queue(records)
        );
    }

    // A run that measured nothing because of a misconfiguration must not read
    // as a run that measured nothing to report.
    for (name, _, records) in builds {
        for r in records {
            if let Record::Problem { device, message } = r {
                println!("  {name}: {}: {message}", elide(device, 40));
            }
        }
    }

    // How long a batch takes to drain is the measurement that catches a
    // completion published late: the queue still comes back whole and in
    // order, just slowly, because a waiter that has already been woken is no
    // longer registered and waits out its timeout before looking again.
    for (name, _, records) in builds {
        for line in drain_lines(Some(name), records) {
            println!("{line}");
        }
    }

    for (name, _, records) in builds {
        let mut latency: Vec<u64> = records
            .iter()
            .filter_map(|r| match r {
                Record::Transfer { latency_us, .. } => Some(*latency_us),
                _ => None,
            })
            .collect();
        print!(
            "{}",
            histogram(
                &format!("completion latency after cancel: {name}"),
                &mut latency
            )
        );
    }
}

// -------------------------------------------------------------------- reporting

fn print_csv(records: &[Record]) {
    println!("device,delay_us,iteration,latency_us,outcome,actual_len");
    for r in records {
        if let Record::Transfer {
            device,
            delay_us,
            iteration,
            latency_us,
            outcome,
            actual_len,
        } = r
        {
            println!("{device},{delay_us},{iteration},{latency_us},{outcome},{actual_len}");
        }
    }
}

fn print_table(records: &[Record], trials: u32, failed: u32, config: &Config) {
    let summary = Summary::of(records, trials, failed);

    println!(
        "{} trials, {} aborted, {} transfers, delays {}",
        summary.trials,
        summary.trials_failed,
        summary.transfers,
        describe_delays(&config.delays_us),
    );
    println!();

    // Per device, so one sick device does not hide behind the others.
    let mut devices: Vec<&String> = Vec::new();
    for r in records {
        if let Record::Transfer { device, .. } = r {
            if !devices.contains(&device) {
                devices.push(device);
            }
        }
    }

    println!(
        "{:<48} {:>7} {:>9} {:>9} {:>9} {:>9}",
        "device", "xfers", "p50 ms", "p90 ms", "p99 ms", "max ms"
    );
    for device in &devices {
        let mut latency: Vec<u64> = records
            .iter()
            .filter_map(|r| match r {
                Record::Transfer {
                    device: d,
                    latency_us,
                    ..
                } if &d == device => Some(*latency_us),
                _ => None,
            })
            .collect();
        let q = Quantiles::of(&mut latency);
        println!(
            "{:<48} {:>7} {:>9} {:>9} {:>9} {:>9}",
            elide(device, 48),
            q.n,
            ms(q.p50),
            ms(q.p90),
            ms(q.p99),
            ms(q.max)
        );
    }

    println!();
    println!("transfer outcomes");
    for (outcome, n) in &summary.outcomes {
        println!("  {outcome:<24} {n:>7}");
    }
    let abandoned: usize = records
        .iter()
        .filter_map(|r| match r {
            Record::Abandoned { transfers, .. } => Some(*transfers),
            _ => None,
        })
        .sum();
    if abandoned > 0 {
        println!("  {:<24} {abandoned:>7}", "abandoned at drop");
    }

    let queue = describe_queue(records);
    if queue != "-" {
        println!();
        println!("batch drain, stalls/reorders: {queue}");
        for line in drain_lines(None, records) {
            println!("{line}");
        }
    }

    let mut latency: Vec<u64> = records
        .iter()
        .filter_map(|r| match r {
            Record::Transfer { latency_us, .. } => Some(*latency_us),
            _ => None,
        })
        .collect();
    print!(
        "{}",
        histogram("completion latency after cancel", &mut latency)
    );

    let mut reopen: Vec<u64> = records
        .iter()
        .filter_map(|r| match r {
            Record::Reopen { elapsed_us, .. } => Some(*elapsed_us),
            _ => None,
        })
        .collect();
    if !reopen.is_empty() {
        print!("{}", histogram("endpoint reopen after drop", &mut reopen));
        let busy = records
            .iter()
            .filter(|r| {
                matches!(
                    r,
                    Record::Reopen {
                        first_error: Some(_),
                        ..
                    }
                )
            })
            .count();
        let stuck = records
            .iter()
            .filter(|r| {
                matches!(
                    r,
                    Record::Reopen {
                        succeeded: false,
                        ..
                    }
                )
            })
            .count();
        println!(
            "  {busy} of {} reopens needed a retry, {stuck} never succeeded",
            reopen.len()
        );
        if let Some(Record::Reopen {
            first_error: Some(e),
            ..
        }) = records.iter().find(|r| {
            matches!(
                r,
                Record::Reopen {
                    first_error: Some(_),
                    ..
                }
            )
        }) {
            println!("  first reopen error: {e}");
        }
    }

    if failed > 0 {
        println!();
        println!("aborted trials");
        for r in records {
            if let Record::TrialFailed {
                trial,
                status,
                message,
            } = r
            {
                println!(
                    "  trial {trial}: {status}{}",
                    message
                        .as_deref()
                        .map(|m| format!("\n    {m}"))
                        .unwrap_or_default()
                );
            }
        }
    }

    let problems: Vec<&Record> = records
        .iter()
        .filter(|r| matches!(r, Record::Problem { .. }))
        .collect();
    if !problems.is_empty() {
        println!();
        println!("problems");
        for r in problems {
            if let Record::Problem { device, message } = r {
                println!("  {}: {message}", elide(device, 40));
            }
        }
    }
}

/// Describe how long a batch took to drain, and what that time is made of.
///
/// A drain scales with the batch when it is working and pins to the timeout
/// when it is waiting, so both bounds are on the line. Per write stays flat
/// across batch sizes when the cost is per transfer.
fn drain_lines(name: Option<&str>, records: &[Record]) -> Vec<String> {
    // Grouped by shape, because a merged stream can hold several. A depth
    // sweep is the usual case, and its per-write figure across depths is the
    // measurement that separates a serialised backend from a pipelined one,
    // so every shape gets its own line rather than being averaged away.
    let mut groups: Vec<((u32, u64), Vec<u64>)> = Vec::new();

    for r in records {
        if let Record::Order {
            drain_us,
            submitted,
            timeout_ms,
            ..
        } = r
        {
            let key = (*submitted, *timeout_ms);
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, drains)) => drains.push(*drain_us),
                None => groups.push((key, vec![*drain_us])),
            }
        }
    }
    groups.sort_by_key(|((submitted, _), _)| *submitted);

    groups
        .into_iter()
        .map(|((submitted, timeout_ms), mut drains)| {
            let q = Quantiles::of(&mut drains);
            let per_write = ms(q.p50 / u64::from(submitted.max(1)));

            // Within one percent of the timeout is waiting, not working.
            let pinned = q.p50 * 100 >= timeout_ms * 1000 * 99;
            let note = if pinned { "  (at timeout)" } else { "" };

            let label = match name {
                Some(name) => format!("batch drain {name}"),
                None => "batch drain".to_string(),
            };
            format!(
                "{label}: {submitted} writes, {timeout_ms} ms timeout: \
                 p50={} p90={} max={} ms, {per_write} ms per write{note}",
                ms(q.p50),
                ms(q.p90),
                ms(q.max),
            )
        })
        .collect()
}

/// Summarise the batch drains as `stalls/reorders of batches`.
fn describe_queue(records: &[Record]) -> String {
    let mut batches = 0usize;
    let mut stalls = 0usize;
    let mut reorders = 0usize;

    for r in records {
        if let Record::Order { order, stalled, .. } = r {
            batches += 1;
            if *stalled {
                stalls += 1;
            }
            if order.iter().enumerate().any(|(i, &tag)| tag != i as u32) {
                reorders += 1;
            }
        }
    }

    if batches == 0 {
        "-".to_string()
    } else {
        format!("{stalls}/{reorders} of {batches}")
    }
}

/// Render a statistic with no samples as absent. A zero would read as an
/// instant result, which is the opposite of the truth.
fn or_absent(n: usize, value: String) -> String {
    if n == 0 {
        "-".to_string()
    } else {
        value
    }
}

fn describe_delays(delays: &[u64]) -> String {
    match delays {
        [] => "none".into(),
        [one] => format!("{one} us"),
        many => format!(
            "{} us to {} us in {} steps",
            many[0],
            many[many.len() - 1],
            many.len()
        ),
    }
}

fn elide(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.to_string();
    }
    // By character, not by byte: a device string is usually a hex serial, but
    // nothing stops it carrying anything a descriptor can hold.
    let keep: String = s.chars().take(width - 3).collect();
    format!("{keep}...")
}

/// A log-scaled histogram, since the shape of interest is a second mode
/// seconds away from the first.
fn histogram(title: &str, values: &mut [u64]) -> String {
    let mut out = String::new();
    if values.is_empty() {
        return out;
    }
    values.sort_unstable();

    // 100 us to 30 s, one bucket per power of ten split in half.
    const EDGES: [u64; 12] = [
        100, 316, 1_000, 3_162, 10_000, 31_623, 100_000, 316_228, 1_000_000, 3_162_278, 10_000_000,
        31_622_777,
    ];
    let mut counts = [0usize; EDGES.len() + 1];
    for &v in values.iter() {
        let bucket = EDGES.iter().position(|&e| v < e).unwrap_or(EDGES.len());
        counts[bucket] += 1;
    }

    let peak = counts.iter().copied().max().unwrap_or(1).max(1);
    let _ = writeln!(out, "\n{title}, ms (n={})", values.len());
    for (i, &n) in counts.iter().enumerate() {
        if n == 0 {
            continue;
        }
        let label = match i {
            0 => format!("<{}", ms(EDGES[0])),
            i if i == EDGES.len() => format!(">={}", ms(EDGES[EDGES.len() - 1])),
            i => ms(EDGES[i - 1]),
        };
        let bar = "#".repeat((n * 40 / peak).max(1));
        let _ = writeln!(out, "  {label:>8} {bar:<40} {n}");
    }
    out
}

/// Every duration this program prints is milliseconds. The values span four
/// orders of magnitude, and a unit that changed with them would make two
/// numbers look comparable when they are not.
fn ms(us: u64) -> String {
    format!("{:.3}", us as f64 / 1000.0)
}
