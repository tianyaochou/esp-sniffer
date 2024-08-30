#![no_std]
#![no_main]
#![feature(ascii_char)]
#![feature(type_alias_impl_trait)]

use core::{cell::RefCell, fmt::Write};

use byte::BytesExt;
use critical_section::Mutex;
use derive_more::From;
use esp_backtrace as _;
use esp_hal::{
    clock::ClockControl,
    delay::Delay,
    gpio::Io,
    peripherals::{Peripherals, UART1},
    prelude::*,
    system::{SoftwareInterrupt, SystemControl},
    uart::{
        config::{AtCmdConfig, Config},
        ClockSource, Uart,
    },
    Blocking,
};
use esp_ieee802154::*;
use esp_println::{print, println};
use ieee802154::mac::{self, FrameSerDesContext};

#[derive(From, Debug)]
enum Error {
    UartError(esp_hal::uart::Error),
    ByteBufferError(byte::Error),
}

static UART: Mutex<RefCell<Option<Uart<'static, UART1, Blocking>>>> =
    Mutex::new(RefCell::new(None));

static WPAN: Mutex<RefCell<Option<Ieee802154>>> = Mutex::new(RefCell::new(None));

#[handler(priority=esp_hal::interrupt::Priority::Priority1)]
fn at_cmd_handler() {
    println!("AT_CMD triggered");
    critical_section::with(|cs| {
        let mut uart = UART.borrow_ref_mut(cs);
        let uart = uart.as_mut().unwrap();
        uart.reset_at_cmd_interrupt();
        let mut at_cmd_buf = [0; 5];
        if let Ok(_) = uart.read_bytes(&mut at_cmd_buf) {
            let cmd = at_cmd_buf[4];
            if cmd == b'R' {
                println!("Resetting...");
                uart.flush_tx();
                uart.write_str("START\n");
            } else if 11 <= cmd && cmd <= 26 {
                println!("Setting channel at {}", cmd);
                let mut _wpan = WPAN.borrow_ref_mut(cs);
                let wpan = _wpan.as_mut().unwrap();
                wpan.set_config(esp_ieee802154::Config {
                    promiscuous: true,
                    rx_when_idle: true,
                    channel: cmd,
                    ..Default::default()
                });
                wpan.start_receive();
            } else {
            }
        }
    });
}

static SI: Mutex<RefCell<Option<SoftwareInterrupt<0>>>> = Mutex::new(RefCell::new(None));

fn wpan_received_handler() {
    critical_section::with(|cs| SI.borrow_ref(cs).as_ref().unwrap().raise());
}

#[handler(priority=esp_hal::interrupt::Priority::Priority2)]
fn si_handler() {
    println!("SI triggered");
    critical_section::with(|cs| {
        SI.borrow_ref(cs).as_ref().unwrap().reset();
        let mut _wpan = WPAN.borrow_ref_mut(cs);
        let wpan = _wpan.as_mut().unwrap();
        if let Some(raw) = wpan.get_raw_received() {
            let mut _uart = UART.borrow_ref_mut(cs);
            let uart = _uart.as_mut().unwrap();
            if let Ok(len) = uart.write_bytes(raw.data.as_slice()) {
                uart.flush_tx();
                println!("Wrote {} bytes", len);
            }
        }
    });
}

#[entry]
fn main() -> ! {
    // === Boilerplate to get started
    let mut peripherals = Peripherals::take();
    let system = SystemControl::new(peripherals.SYSTEM);

    let clocks = ClockControl::max(system.clock_control).freeze();
    let delay = Delay::new(&clocks);
    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);

    esp_println::logger::init_logger_from_env();

    // === Init UART
    const baud_rate: u32 = 460800;
    let (mut tx_pin, mut rx_pin) = (io.pins.gpio7, io.pins.gpio6);
    let mut uart = Uart::new(peripherals.UART1, &clocks, tx_pin, rx_pin).expect("Uart Init Failed");
    uart.set_at_cmd(AtCmdConfig {
        pre_idle_count: None,
        post_idle_count: Some(0),
        gap_timeout: None,
        cmd_char: b'+',
        char_num: Some(4),
    });
    uart.set_interrupt_handler(at_cmd_handler);
    uart.listen_at_cmd();
    uart.change_baud(baud_rate, ClockSource::Xtal, &clocks);
    // let (mut uart_tx, uart_rx) = uart.split();
    critical_section::with(|cs| UART.replace(cs, Some(uart)));

    // === Set up software interrupt
    // Cannot access Ieee802154 inside its callback, so we need to set up a software interrupt
    let mut si = system.software_interrupt_control.software_interrupt0;
    si.set_interrupt_handler(si_handler);
    critical_section::with(|cs| SI.replace(cs, Some(si)));

    // === Init IEEE802.15.4
    let channels = 11..=26u8; // a total of 16 IEEE802.15.4 channels at 2.4GHz

    let channel = 15;
    let ieee802154_config = esp_ieee802154::Config {
        promiscuous: true,
        rx_when_idle: true,
        channel,
        ..esp_ieee802154::Config::default()
    };
    let mut ieee802154 = Ieee802154::new(peripherals.IEEE802154, &mut peripherals.RADIO_CLK);
    ieee802154.set_config(ieee802154_config);
    ieee802154.set_rx_available_callback_fn(wpan_received_handler);
    ieee802154.start_receive();
    critical_section::with(|cs| WPAN.replace(cs, Some(ieee802154)));

    // === Main Loop
    loop {}
}
