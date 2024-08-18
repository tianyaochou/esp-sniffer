#![no_std]
#![no_main]
#![feature(ascii_char)]
#![feature(type_alias_impl_trait)]

use core::{
    borrow::{Borrow, BorrowMut},
    cell::{Cell, RefCell},
    fmt::Write,
    panic,
    ptr::addr_of_mut,
};

use byte::BytesExt;
use critical_section::Mutex;
use derive_more::From;
use esp_backtrace as _;
use esp_hal::{
    clock::ClockControl,
    delay::Delay,
    gpio::Io,
    interrupt::InterruptHandler,
    peripherals::{Peripherals, UART0, UART1},
    prelude::*,
    rtc_cntl::sleep::Uart1WakeupSource,
    system::SystemControl,
    uart::{ClockSource, Uart, UartRx},
    Blocking,
};
use esp_ieee802154::*;
use esp_println::println;
use ieee802154::mac::{self, FrameSerDesContext};

#[derive(From, Debug)]
enum Error {
    UartError(esp_hal::uart::Error),
    ByteBufferError(byte::Error),
}

fn dump_frame<T, M>(uart: &mut Uart<T, M>, frame: &Frame) -> Result<(), Error>
where
    T: esp_hal::uart::Instance,
    M: esp_hal::Mode,
{
    let frm = mac::Frame {
        header: frame.header,
        content: frame.content,
        payload: frame.payload.as_slice(),
        footer: frame.footer,
    };
    let mut buffer = [0u8; 127];
    let mut len = 0;
    buffer.write_with(
        &mut len,
        frm,
        &mut FrameSerDesContext::no_security(mac::FooterMode::Explicit),
    )?;
    uart.write_bytes(&buffer)?;
    Ok(())
}

static UART: Mutex<RefCell<Option<UartRx<'static, UART1, Blocking>>>> =
    Mutex::new(RefCell::new(None));

static WPAN: Mutex<RefCell<Option<Ieee802154>>> = Mutex::new(RefCell::new(None));

#[handler]
fn at_cmd_handler() {
    critical_section::with(|cs| {
        let mut uart = UART.borrow_ref_mut(cs);
        let uart = uart.as_mut().unwrap();
        if let Ok(channel) = uart.read_byte() {
            WPAN.borrow_ref_mut(cs)
                .take()
                .unwrap()
                .set_config(esp_ieee802154::Config {
                    promiscuous: true,
                    rx_when_idle: true,
                    channel,
                    ..Default::default()
                })
        }
    });
}

// fn wpan_received_handler() {
//     critical_section::with(|cs| {
//         let mut wpan = WPAN.borrow_ref_mut(cs);
//         let wpan = wpan.as_mut().unwrap().get_raw_received();
//         let mut uart = UART.borrow_ref_mut(cs);
//         let uart = uart.as_mut().unwrap();
//         if let Some(raw) = wpan {
//             uart.write_bytes(raw.data.as_slice());
//         }
//     })
// }

#[entry]
fn main() -> ! {
    // === Boilerplate to get started
    let mut peripherals = Peripherals::take();
    let system = SystemControl::new(peripherals.SYSTEM);

    let clocks = ClockControl::max(system.clock_control).freeze();
    let delay = Delay::new(&clocks);
    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);

    // esp_println::logger::init_logger_from_env();

    // === Init UART
    const baud_rate: u32 = 460800;
    let (mut tx_pin, mut rx_pin) = (io.pins.gpio7, io.pins.gpio6);
    let mut uart = Uart::new(peripherals.UART1, &clocks, tx_pin, rx_pin).expect("Uart Init Failed");
    uart.change_baud(baud_rate, ClockSource::Xtal, &clocks);
    let (mut uart_tx, uart_rx) = uart.split();
    critical_section::with(|cs| UART.replace(cs, Some(uart_rx)));

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
    ieee802154.start_receive();

    // === Main Loop
    loop {
        if let Some(raw) = ieee802154.get_raw_received() {
            println!("Received");
            if let Ok(len) = uart_tx.write_bytes(raw.data.as_slice()) {
                uart_tx.flush_tx();
                println!("Write success, len: {}", len);
                println!("{:?}", raw.data);
            } else {
                println!("Write failed");
            }
        }
    }
}
