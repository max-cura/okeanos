use bcm2835_lpa::{SYSTMR, UART1};
use crate::{boot_umsg, staging, uart1};
use crate::fmt::UartWrite;
use core::fmt::Write;
use theseus_common::theseus::v1::{DEVICE_PROTOCOL_RESET_TIMEOUT, MessageContent, RETRY_ATTEMPTS_CAP};
use crate::delay::STInstant;
use crate::staging::{Integrity, relocate_stub_inner, RelocationConfig, RelocationParams};
use crate::theseus::{uart1_send_theseus_packet, uart1_wait_for_theseus_packet, WaitPacketError};

// TODO: find a more reasonable size for these two
pub const CTL_BUF_SIZE : usize = 0x200;
// for now, get it in 4-byte chunks
const CHUNK_SIZE : usize = 0x100;

pub(crate) fn perform_download(
    uw: &mut UartWrite,
    uart: &UART1,
    st: &SYSTMR
) {
    #[derive(Debug, Copy, Clone)]
    enum S {
        // sending RequestProgramInfoRPC, waiting for:
        //  - SetBaudRateRPC or ProgramInfo
        // transitions to:
        //  - SettingBaudRate or ProgramRequest
        InitialProgramInfoRequest,
        // sending BaudRateAck, waiting for:
        //  - BaudRateReady
        // transitions to:
        //  - (EXIT) if possible:false
        //  - SecondProgramInfoReuquest
        SettingBaudRate { baud_rate: u32 },
        // sending RequestProgramInfoRPC, waiting for:
        //  - ProgramInfo
        // transitions to:
        //  - ProgramRequest
        SecondProgramInfoRequest,
        // sending ProgramRequest, waiting for:
        //  - ProgramReady
        // transitions to:
        //  - ChunkRequest
        ProgramRequest,
        // sending ReadyForChunk, waiting for:
        //  - ProgramChunk
        // transitions to:
        //  - ChunkRequest OR FinishedReceiving
        ChunkRequest { chunk_no: usize },
        // sending ProgramReceived, waiting for:
        //  - (NOTHING)
        // transitions to:
        //  - (EXIT)
        FinishedReceiving,
    }
    impl S {
        pub fn waiting_for(&self) -> &'static str {
            match self {
                S::InitialProgramInfoRequest => { "SetBaudRateRPC or ProgramInfo" }
                S::SettingBaudRate { .. } => { "BaudRateReady" }
                S::SecondProgramInfoRequest => { "ProgramInfo" }
                S::ProgramRequest => { "ProgramReady" }
                S::ChunkRequest {..} => { "ProgramChunk" }
                S::FinishedReceiving => { "(Nothing - this message should never print)" }
            }
        }
    }

    boot_umsg!(uw, "[theseus-device]: using THESEUSv1 protocol.");

    boot_umsg!(uw, "[theseus-device]: now={}", crate::delay::st_read(st));
    boot_umsg!(uw, "[theseus-device]: now={}", crate::delay::st_read(st));
    boot_umsg!(uw, "[theseus-device]: now={}", crate::delay::st_read(st));

    // the outgoing messages in v1 are really quite small (except PrintMessageRPC but that's handled
    // in fmt.rs).
    let mut out_buf = [0; 0x20];
    // incoming will be larger due to ProgramChunk, but we control chunk size.
    let mut in_buf = [0; CTL_BUF_SIZE];
    let mut last_received_packet = STInstant::now(st);
    // just finished protocol, so we're broadcasting RequestProgramInfo in Initial Mode
    let mut state = S::InitialProgramInfoRequest;
    let mut outgoing_msg = MessageContent::RequestProgramInfoRPC;
    // number of RECEIVE_TIMEOUTs/wait_for_theseus_packet calls since last catching ANYTHING (not
    // necessarily a valid packet, just... bytes. Not even necessarily a valid packet, just... some
    // sequence of COBS-encoded bytes with a valid CRC.
    let mut retries_since_last_received = 0;

    let mut relocation_config = RelocationConfig::new();
    let mut needs_to_relocate = false;

    #[derive(Debug, Copy, Clone)]
    struct ProgInfo {
        load_at_address: usize,
        /// Length of DEFLATED
        transmission_length: usize,
        /// CRC32 of entire INFLATED program
        crc: u32,
    }
    let mut prog_info = ProgInfo {
        load_at_address: 0,
        transmission_length: 0,
        crc: 0,
    };
    let mut last_chunk = 0;

    'outer: loop {
        // if last_received_packet.elapsed(st) > DEVICE_PROTOCOL_RESET_TIMEOUT {
        //     boot_umsg!(uw, "[theseus-device]: no valid packet within {}us, aborting.",
        //         DEVICE_PROTOCOL_RESET_TIMEOUT.as_micros());
        //     return
        // }

        send_v1_message(uart, &outgoing_msg, &mut out_buf);

        in_buf.iter_mut().for_each(|v| *v = 0);

        // unlike in theseus-upload, uart1_wait_for_theseus_packet will grab at most a single packet
        let message_raw = match uart1_wait_for_theseus_packet(
            uw, uart, st, &mut in_buf
        ) {
            Ok(l) => {
                retries_since_last_received = 0;
                &in_buf[0..l]
            }
            Err(WaitPacketError::Timeout) => {
                if retries_since_last_received == RETRY_ATTEMPTS_CAP {
                    boot_umsg!(uw, "[theseus-device]: failed to read after {retries_since_last_received} retries, aborting.");
                    // download failed
                    return
                }
                // retries_since_last_received += 1;
                // this is 3600us : big fat nope, screws with the timing while at the same time not
                // being a real error state; thus, discard and continue
                //boot_umsg!(uw, "[theseus-device]: hit read timeout, retrying ({retries_since_last_received}/{})", RETRY_ATTEMPTS_CAP);
                continue
            }
            Err(WaitPacketError::RecvError(e)) => {
                retries_since_last_received += 1;
                boot_umsg!(uw, "[theseus-device]: failed to receive packet: {e}, retrying \
                                ({retries_since_last_received}/{}), buffer: \
                    {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
                                RETRY_ATTEMPTS_CAP,
in_buf[0], in_buf[1], in_buf[2], in_buf[3], in_buf[4], in_buf[5], in_buf[6], in_buf[7], in_buf[8], in_buf[9], in_buf[10], in_buf[11], in_buf[12], in_buf[13], in_buf[14], in_buf[15], in_buf[16], in_buf[17], in_buf[18], in_buf[19], in_buf[20], in_buf[21], in_buf[22], in_buf[23], in_buf[24], in_buf[25], in_buf[26], in_buf[27], in_buf[28], in_buf[29], in_buf[30], in_buf[31], in_buf[32], in_buf[33], in_buf[34], in_buf[35], in_buf[36], in_buf[37], in_buf[38], in_buf[39], in_buf[40], in_buf[41], in_buf[42], in_buf[43], in_buf[44], in_buf[45], in_buf[46], in_buf[47], in_buf[48], in_buf[49], in_buf[50], in_buf[51], in_buf[52], in_buf[53], in_buf[54], in_buf[55], in_buf[56], in_buf[57], in_buf[58], in_buf[59], in_buf[60], in_buf[61], in_buf[62], in_buf[63], in_buf[64], in_buf[65], in_buf[66], in_buf[67], in_buf[68], in_buf[69], in_buf[70], in_buf[71], in_buf[72], in_buf[73], in_buf[74], in_buf[75], in_buf[76], in_buf[77], in_buf[78], in_buf[79], in_buf[80], in_buf[81], in_buf[82], in_buf[83], in_buf[84], in_buf[85], in_buf[86], in_buf[87], in_buf[88], in_buf[89], in_buf[90], in_buf[91], in_buf[92], in_buf[93], in_buf[94], in_buf[95], in_buf[96], in_buf[97], in_buf[98], in_buf[99], in_buf[100], in_buf[101], in_buf[102], in_buf[103], in_buf[104], in_buf[105], in_buf[106], in_buf[107], in_buf[108], in_buf[109], in_buf[110], in_buf[111], in_buf[112], in_buf[113], in_buf[114], in_buf[115], in_buf[116], in_buf[117], in_buf[118], in_buf[119], in_buf[120], in_buf[121], in_buf[122], in_buf[123], in_buf[124], in_buf[125], in_buf[126], in_buf[127], in_buf[128], in_buf[129], in_buf[130], in_buf[131], in_buf[132], in_buf[133], in_buf[134], in_buf[135], in_buf[136], in_buf[137], in_buf[138], in_buf[139], in_buf[140], in_buf[141], in_buf[142], in_buf[143], in_buf[144], in_buf[145], in_buf[146], in_buf[147], in_buf[148], in_buf[149], in_buf[150], in_buf[151], in_buf[152], in_buf[153], in_buf[154], in_buf[155], in_buf[156], in_buf[157], in_buf[158], in_buf[159], in_buf[160], in_buf[161], in_buf[162], in_buf[163], in_buf[164], in_buf[165], in_buf[166], in_buf[167], in_buf[168], in_buf[169], in_buf[170], in_buf[171], in_buf[172], in_buf[173], in_buf[174], in_buf[175], in_buf[176], in_buf[177], in_buf[178], in_buf[179], in_buf[180], in_buf[181], in_buf[182], in_buf[183], in_buf[184], in_buf[185], in_buf[186], in_buf[187], in_buf[188], in_buf[189], in_buf[190], in_buf[191], in_buf[192], in_buf[193], in_buf[194], in_buf[195], in_buf[196], in_buf[197], in_buf[198], in_buf[199], in_buf[200], in_buf[201], in_buf[202], in_buf[203], in_buf[204], in_buf[205], in_buf[206], in_buf[207], in_buf[208], in_buf[209], in_buf[210], in_buf[211], in_buf[212], in_buf[213], in_buf[214], in_buf[215], in_buf[216], in_buf[217], in_buf[218], in_buf[219], in_buf[220], in_buf[221], in_buf[222], in_buf[223], in_buf[224], in_buf[225], in_buf[226], in_buf[227], in_buf[228], in_buf[229], in_buf[230], in_buf[231], in_buf[232], in_buf[233], in_buf[234], in_buf[235], in_buf[236], in_buf[237], in_buf[238], in_buf[239], in_buf[240], in_buf[241], in_buf[242], in_buf[243], in_buf[244], in_buf[245], in_buf[246], in_buf[247], in_buf[248], in_buf[249], in_buf[250], in_buf[251], in_buf[252], in_buf[253], in_buf[254], in_buf[255], in_buf[256], in_buf[257], in_buf[258], in_buf[259], in_buf[260], in_buf[261], in_buf[262], in_buf[263], in_buf[264], in_buf[265], in_buf[266], in_buf[267], in_buf[268], in_buf[269], in_buf[270], in_buf[271], in_buf[272], in_buf[273], in_buf[274], in_buf[275], in_buf[276], in_buf[277], in_buf[278], in_buf[279], in_buf[280], in_buf[281], in_buf[282], in_buf[283], in_buf[284], in_buf[285], in_buf[286], in_buf[287], in_buf[288], in_buf[289], in_buf[290], in_buf[291], in_buf[292], in_buf[293], in_buf[294], in_buf[295], in_buf[296], in_buf[297], in_buf[298], in_buf[299], in_buf[300], in_buf[301], in_buf[302], in_buf[303], in_buf[304], in_buf[305], in_buf[306], in_buf[307], in_buf[308], in_buf[309], in_buf[310], in_buf[311], in_buf[312], in_buf[313], in_buf[314], in_buf[315], in_buf[316], in_buf[317], in_buf[318], in_buf[319], in_buf[320], in_buf[321], in_buf[322], in_buf[323], in_buf[324], in_buf[325], in_buf[326], in_buf[327], in_buf[328], in_buf[329], in_buf[330], in_buf[331], in_buf[332], in_buf[333], in_buf[334], in_buf[335], in_buf[336], in_buf[337], in_buf[338], in_buf[339], in_buf[340], in_buf[341], in_buf[342], in_buf[343], in_buf[344], in_buf[345], in_buf[346], in_buf[347], in_buf[348], in_buf[349], in_buf[350], in_buf[351], in_buf[352], in_buf[353], in_buf[354], in_buf[355], in_buf[356], in_buf[357], in_buf[358], in_buf[359], in_buf[360], in_buf[361], in_buf[362], in_buf[363], in_buf[364], in_buf[365], in_buf[366], in_buf[367], in_buf[368], in_buf[369], in_buf[370], in_buf[371], in_buf[372], in_buf[373], in_buf[374], in_buf[375], in_buf[376], in_buf[377], in_buf[378], in_buf[379], in_buf[380], in_buf[381], in_buf[382], in_buf[383], in_buf[384], in_buf[385], in_buf[386], in_buf[387], in_buf[388], in_buf[389], in_buf[390], in_buf[391], in_buf[392], in_buf[393], in_buf[394], in_buf[395], in_buf[396], in_buf[397], in_buf[398], in_buf[399], in_buf[400], in_buf[401], in_buf[402], in_buf[403], in_buf[404], in_buf[405], in_buf[406], in_buf[407], in_buf[408], in_buf[409], in_buf[410], in_buf[411], in_buf[412], in_buf[413], in_buf[414], in_buf[415], in_buf[416], in_buf[417], in_buf[418], in_buf[419], in_buf[420], in_buf[421], in_buf[422], in_buf[423], in_buf[424], in_buf[425], in_buf[426], in_buf[427], in_buf[428], in_buf[429], in_buf[430], in_buf[431], in_buf[432], in_buf[433], in_buf[434], in_buf[435], in_buf[436], in_buf[437], in_buf[438], in_buf[439], in_buf[440], in_buf[441], in_buf[442], in_buf[443], in_buf[444], in_buf[445], in_buf[446], in_buf[447], in_buf[448], in_buf[449], in_buf[450], in_buf[451], in_buf[452], in_buf[453], in_buf[454], in_buf[455], in_buf[456], in_buf[457], in_buf[458], in_buf[459], in_buf[460], in_buf[461], in_buf[462], in_buf[463], in_buf[464], in_buf[465], in_buf[466], in_buf[467], in_buf[468], in_buf[469], in_buf[470], in_buf[471], in_buf[472], in_buf[473], in_buf[474], in_buf[475], in_buf[476], in_buf[477], in_buf[478], in_buf[479], in_buf[480], in_buf[481], in_buf[482], in_buf[483], in_buf[484], in_buf[485], in_buf[486], in_buf[487], in_buf[488], in_buf[489], in_buf[490], in_buf[491], in_buf[492], in_buf[493], in_buf[494], in_buf[495], in_buf[496], in_buf[497], in_buf[498], in_buf[499], in_buf[500], in_buf[501], in_buf[502], in_buf[503], in_buf[504], in_buf[505], in_buf[506], in_buf[507], in_buf[508], in_buf[509], in_buf[510], in_buf[511],

                    );
                    // super::hexify(&in_buf[..]).as_str());
                continue
            }
        };
        let Ok(message) = postcard::from_bytes::<MessageContent>(message_raw)
            .inspect_err(|e| {
                boot_umsg!(uw, "[theseus-device]: failed to deserialize packet: {e}");
            }) else { continue };
        // okay, packet deserialized safely, so reset last_received_packet
        last_received_packet = STInstant::now(st);

        match message {
            /* TODO: SETBAUDRATE */
            MessageContent::SetBaudRateRPC { baud_rate } => {
                match state {
                    S::InitialProgramInfoRequest => {
                        boot_umsg!(uw, "Received SetBaudRateRPC")
                    }
                    S::SettingBaudRate { baud_rate: desired_baud_rate } => {
                        if baud_rate == desired_baud_rate {
                            // idempotent
                            continue
                        } else {
                            boot_umsg!(uw, "[theseus-device]: received conflicting SetBaudRateRPC \
                                            for {desired_baud_rate} (1st) and {baud_rate} (2nd), \
                                            aborting.");
                            return
                        }
                    }
                    _ => { boot_umsg!(uw, "[theseus-device]: device received unexpected {message:?}\
                                           while waiting for {}", state.waiting_for()) }
                }
            }
            MessageContent::BaudRateReady => {
                boot_umsg!(uw, "[theseus-device]: received BaudRateReady")
            }
            /* END TODO: SETBAUDRATE */
            MessageContent::ProgramInfo { load_at_address, program_size, program_crc32 } => {
                match state {
                    S::InitialProgramInfoRequest => {
                        boot_umsg!(uw, "[theseus-device]: transition InitialProgramInfoRequest -> ProgramRequest");
                        prog_info = ProgInfo {
                            load_at_address: load_at_address as usize,
                            transmission_length: program_size as usize,
                            crc: program_crc32,
                        };
                        boot_umsg!(uw, "[theseus-device]: prog_info={prog_info:?}");
                        relocation_config = crate::staging::calculate(load_at_address as usize, program_size as usize);
                        if relocation_config.relocate_first_n_bytes > 0 {
                            needs_to_relocate = true;
                        }
                        boot_umsg!(uw, "[theseus-device]: relocation config=[DL={:#010x},SBL={:#010x},RFNB={},SL={:#010x}]",
                            relocation_config.desired_location,
                            relocation_config.side_buffer_location,
                            relocation_config.relocate_first_n_bytes,
                            relocation_config.stub_location);
                        state = S::ProgramRequest;
                        outgoing_msg = MessageContent::RequestProgramRPC {
                            crc_retransmission: program_crc32,
                            chunk_size: CHUNK_SIZE as u32,
                        };
                        let num_chunks = (program_size as usize + CHUNK_SIZE - 1) / CHUNK_SIZE;
                        last_chunk = num_chunks - 1;
                    }
                    S::ProgramRequest => {
                        // dump -> overflow from prior state
                        continue
                    }
                    _ => { boot_umsg!(uw, "[theseus-device]: device received unexpected {message:?}\
                                           while waiting for {}", state.waiting_for()) }
                }
            }
            MessageContent::ProgramReady => {
                match state {
                    S::ProgramRequest => {
                        boot_umsg!(uw, "[theseus-device]: transition ProgramRequest -> ChunkRequest");
                        state = S::ChunkRequest { chunk_no: 0 };
                        outgoing_msg = MessageContent::ReadyForChunk {
                            chunk_no: 0,
                        };
                    }
                    S::ChunkRequest {..} => {
                        // dump -> overflow from prior state
                        continue
                    }
                    _ => { boot_umsg!(uw, "[theseus-device]: device received unexpected {message:?}\
                                           while waiting for {}", state.waiting_for()) }
                }
            }
            MessageContent::ProgramChunk { chunk_no, data } => {
                let chunk_no = chunk_no as usize;
                match state {
                    S::ChunkRequest { chunk_no: requested_chunk_no } => {
                        if chunk_no == requested_chunk_no {
                            boot_umsg!(uw, "[theseus-device]: received chunk {chunk_no}/{last_chunk} len={} data=[{} {} {} {}]",
                                data.len(),
                                data.get(0).copied().unwrap_or(0xff),
                                data.get(1).copied().unwrap_or(0xff),
                                data.get(2).copied().unwrap_or(0xff),
                                data.get(3).copied().unwrap_or(0xff));
                        } else {
                            boot_umsg!(uw, "[theseus-device]: got wrong chunk: requested {} got {}", requested_chunk_no, chunk_no);
                            continue
                        }

                        staging::write_bytes_with_relocation(
                            &relocation_config,
                            prog_info.load_at_address + chunk_no * CHUNK_SIZE,
                            data
                        );
                        if requested_chunk_no < last_chunk {
                            outgoing_msg = MessageContent::ReadyForChunk {
                                chunk_no: (requested_chunk_no + 1) as u32
                            };
                            state = S::ChunkRequest {
                                chunk_no: requested_chunk_no + 1
                            };
                        } else {
                            outgoing_msg = MessageContent::ProgramReceived;
                            state = S::FinishedReceiving;

                            // check CRC

                            boot_umsg!(uw, "[theseus-device]: received program, chceking integrity.");

                            match staging::verify_integrity(
                                uw,
                                &relocation_config,
                                prog_info.crc,
                                // TODO: GZIP: needs to change
                                prog_info.transmission_length,
                            ) {
                                Integrity::Ok => { boot_umsg!(uw, "[theseus-device]: crc okay, booting.") }
                                Integrity::CrcMismatch { expected, got } => {
                                    boot_umsg!(uw, "[theseus-device]: crc mismatch: expected {expected}, got {got}; aborting.");
                                    return
                                }
                            }

                            unsafe {
                                relocate_stub_inner(
                                    RelocationParams {
                                        uw,
                                        uart,
                                        stub_dst: relocation_config.stub_location as *mut u8,
                                        prog_dst: prog_info.load_at_address as *mut u8,
                                        prog_src: relocation_config.side_buffer_location as *mut u8,
                                        prog_len: relocation_config.relocate_first_n_bytes,
                                        entry: prog_info.load_at_address as *mut u8,
                                    },
                                    move |_| send_v1_message(uart, &outgoing_msg, &mut out_buf)
                                )
                            }

                            break
                        }
                    }
                    _ => { boot_umsg!(uw, "[theseus-device]: device received unexpected {message:?}\
                                           while waiting for {}", state.waiting_for()) }
                }
            }

            MessageContent::SetProtocolVersion { .. }
            | MessageContent::PrintMessageRPC { .. }
            | MessageContent::BaudRateAck { .. }
            | MessageContent::RequestProgramInfoRPC
            | MessageContent::RequestProgramRPC { .. }
            | MessageContent::ReadyForChunk { .. }
            | MessageContent::ProgramReceived
            => {
                boot_umsg!(uw,"[theseus-device]: device received unexpected {message:?}: message type is \
                               Device->Host only.");
            }
        }
    }
}



fn send_v1_message(
    uart: &UART1,
    message: &MessageContent,
    encoding_buffer: &mut [u8],
) {
    // we don't handle PrintMessageRPC - that's the job of others
    let bytes = match message {
        &MessageContent::PrintMessageRPC { message } => {
            let _ = UartWrite::new(uart).write_str(message);
            return
        }
        message @ _ => {
            postcard::to_slice(message, encoding_buffer)
                // TODO: izzis ok?
                .unwrap()
        }
    };
    uart1_send_theseus_packet(uart, bytes);
}