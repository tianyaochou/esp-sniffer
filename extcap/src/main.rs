#![feature(bufread_skip_until)]
use std::{
    fs::File,
    io::{stdout, BufRead, BufReader, Read},
    thread::sleep,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use pcap_file::pcap::*;
use serialport::{available_ports, new, Result, SerialPort, SerialPortBuilder};

fn main() -> anyhow::Result<()> {
    let mut pcap_writer = PcapWriter::with_header(
        stdout(),
        PcapHeader {
            version_major: 2,
            version_minor: 4,
            ts_correction: 0,
            ts_accuracy: 0,
            snaplen: 65535,
            datalink: pcap_file::DataLink::IEEE802_15_4,
            ts_resolution: pcap_file::TsResolution::MicroSecond,
            endianness: pcap_file::Endianness::Little,
        },
    )?;
    let mut port = serialport::new("/dev/tty.usbserial-14400", 460800).open()?;
    let mut packet_buffer = [0; 129];
    port.write(b"++++R");
    port.flush();
    // sleep(Duration::from_millis(100));
    // port.write(b"R");
    // port.flush();
    let mut b = [0; 1];
    let mut port = BufReader::new(port);
    let mut buffer = String::new();
    port.read_line(&mut buffer);
    loop {
        if let Ok(_) = port.read_exact(&mut packet_buffer) {
            let packet_len = packet_buffer[0];
            pcap_writer.write_packet(&PcapPacket {
                timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?,
                orig_len: 129,
                data: (&packet_buffer[1..(1 + packet_len as usize)]).into(),
            })?;
        }
    }
}
