use super::obd2::DTCCategory;
use crate::can::protocol::isotp::{DemuxContext, ISOTPService, SERVICE_OFFSET, isotp_subsystem};

use strum::{FromRepr, VariantArray};

pub const SUPPRESS_POS_RSP_BIT: u8 = 0x80;

isotp_subsystem!(UDSSubsystem, UDSService, "UDS");

#[repr(u8)]
#[derive(Debug, Clone, Copy, FromRepr, PartialEq, Eq, VariantArray)]
pub enum UDSService {
    DiagnosticSessionControl = 0x10,
    ECUReset = 0x11,
    SecurityAccess = 0x27,
    CommunicationControl = 0x28,
    Authentication = 0x29,
    TesterPresent = 0x3E,
    ControlDTCSettings = 0x85,
    ResponseOnEvent = 0x86,
    LinkControl = 0x87,

    ReadDataByIdentifier = 0x22,
    ReadMemoryByAddress = 0x23,
    ReadScalingDataByIdentifier = 0x24,
    ReadDataByPeriodicIdentifier = 0x2A,
    DynamicallyDefineDataIdentifier = 0x2C,
    WriteDataByIdentifier = 0x2E,
    WriteMemoryByAddress = 0x3D,

    ClearDiagnosticInformation = 0x14,
    ReadDTCInformation = 0x19,

    InputOutputControlByIdentifier = 0x2F,

    RoutineControl = 0x31,

    RequestDownload = 0x34,
    RequestUpload = 0x35,
    TransferData = 0x36,
    RequestTransferExit = 0x37,
    RequestFileTransfer = 0x38,

    SecuredDataTransmission = 0x84,

    NegativeResponce = 0x7F - SERVICE_OFFSET,
}

#[repr(u8)]
#[derive(Debug, Clone, FromRepr, PartialEq, Eq)]
pub enum UDSNegativeResponce {
    GeneralReject = 0x10,
    ServiceNotSupported = 0x11,
    SubFunctionNotSupported = 0x12,
    IncorrectMessageLengthOrInvalidFormat = 0x13,
    ResponseTooLong = 0x14,
    BusyRepeatRequest = 0x21,
    ConditionsNotCorrect = 0x22,
    RequestSequenceError = 0x24,
    NoResponseFromSubnetComponent = 0x25,
    FailurePreventsExecutionOfRequestedAction = 0x26,
    RequestOutOfRange = 0x31,
    SecurityAccessDenied = 0x33,
    AuthenticationRequired = 0x34,
    InvalidKey = 0x35,
    ExceedNumberOfAttempts = 0x36,
    RequiredTimeDelayNotExpired = 0x37,
    SecureDataTransmissionRequired = 0x38,
    SecureDataTransmissionNotAllowed = 0x39,
    SecureDataVerificationFailed = 0x3A,
    CertificateVerificationFailedInvalidTimePeriod = 0x50,
    CertificateVerificationFailedInvalidSignature = 0x51,
    CertificateVerificationFailedInvalidChainOfTrust = 0x52,
    CertificateVerificationFailedInvalidType = 0x53,
    CertificateVerificationFailedInvalidFormat = 0x54,
    CertificateVerificationFailedInvalidContent = 0x55,
    CertificateVerificationFailedInvalidScope = 0x56,
    CertificateVerificationFailedInvalidCertificateRevoked = 0x57,
    OwnershipVerificationFailed = 0x58,
    ChallengeCalculationFailed = 0x59,
    SettingAccessRightsFailed = 0x5A,
    SessionKeyCreationDerivationFailed = 0x5B,
    ConfigurationDataUsageFailed = 0x5C,
    DeAuthenticationFailed = 0x5D,
    UploadDownloadNotAccepted = 0x70,
    TransferDataSuspended = 0x71,
    GeneralProgrammingFailure = 0x72,
    WrongBlockSequenceCounter = 0x73,
    RequestCorrectlyReceivedResponsePending = 0x78,
    SubFunctionNotSupportedInActiveSession = 0x7E,
    ServiceNotSupportedInActiveSession = 0x7F,
    RpmTooHigh = 0x81,
    RpmTooLow = 0x82,
    EngineIsRunning = 0x83,
    EngineIsNotRunning = 0x84,
    EngineRunTimeTooLow = 0x85,
    TemperatureTooHigh = 0x86,
    TemperatureTooLow = 0x87,
    VehicleSpeedTooHigh = 0x88,
    VehicleSpeedTooLow = 0x89,
    ThrottlePedalTooHigh = 0x8A,
    ThrottlePedalTooLow = 0x8B,
    TransmissionRangeNotInNeutral = 0x8C,
    TransmissionRangeNotInGear = 0x8D,
    BrakeSwitchNotClosed = 0x8F,
    ShifterLeverNotInPark = 0x90,
    TorqueConverterClutchLocked = 0x91,
    VoltageTooHigh = 0x92,
    VoltageTooLow = 0x93,
    ResourceTemporarilyNotAvailable = 0x94,
}

/// DiagnosticSessionControl (0x10) sub-function — the requested session.
#[repr(u8)]
#[derive(Debug, FromRepr)]
pub enum DiagnosticSessionControlType {
    Default = 0x01,
    Programming = 0x02,
    ExtendedDiagnostic = 0x03,
    SafetySystemDiagnostic = 0x04,
}

/// ECUReset (0x11) sub-function.
#[repr(u8)]
#[derive(Debug, FromRepr)]
pub enum ECUResetType {
    HardReset = 0x01,
    KeyOffOnReset = 0x02,
    SoftReset = 0x03,
    EnableRapidPowerShutDown = 0x04,
    DisableRapidPowerShutDown = 0x05,
}

/// CommunicationControl (0x28) sub-function.
#[repr(u8)]
#[derive(Debug, FromRepr)]
pub enum CommunicationControlType {
    EnableRxAndTx = 0x00,
    EnableRxAndDisableTx = 0x01,
    DisableRxAndEnableTx = 0x02,
    DisableRxAndTx = 0x03,
    EnableRxAndDisableTxWithEnhancedAddressInformation = 0x04,
    EnableRxAndTxWithEnhancedAddressInformation = 0x05,
}

/// ControlDTCSettings (0x85) sub-function.
#[repr(u8)]
#[derive(Debug, FromRepr)]
pub enum DTCSettingType {
    On = 0x01,
    Off = 0x02,
}

/// RoutineControl (0x31) sub-function.
#[repr(u8)]
#[derive(Debug, FromRepr)]
pub enum RoutineControlType {
    StartRoutine = 0x01,
    StopRoutine = 0x02,
    RequestRoutineResults = 0x03,
}

/// DynamicallyDefineDataIdentifier (0x2C) sub-function.
#[repr(u8)]
#[derive(Debug, FromRepr)]
pub enum DynamicDefinitionType {
    DefineByIdentifier = 0x01,
    DefineByMemoryAddress = 0x02,
    ClearDynamicallyDefinedDataIdentifier = 0x03,
}

/// ReadDTCInformation (0x19) sub-function — the report type.
#[repr(u8)]
#[derive(Debug, FromRepr)]
pub enum DTCReportType {
    ReportNumberOfDTCByStatusMask = 0x01,
    ReportDTCByStatusMask = 0x02,
    ReportDTCSnapshotIdentification = 0x03,
    ReportDTCSnapshotRecordByDTCNumber = 0x04,
    ReportDTCStoredDataByRecordNumber = 0x05,
    ReportDTCExtDataRecordByDTCNumber = 0x06,
    ReportNumberOfDTCBySeverityMaskRecord = 0x07,
    ReportDTCBySeverityMaskRecord = 0x08,
    ReportSeverityInformationOfDTC = 0x09,
    ReportSupportedDTC = 0x0A,
    ReportFirstTestFailedDTC = 0x0B,
    ReportFirstConfirmedDTC = 0x0C,
    ReportMostRecentTestFailedDTC = 0x0D,
    ReportMostRecentConfirmedDTC = 0x0E,
    ReportMirrorMemoryDTCByStatusMask = 0x0F,
    ReportMirrorMemoryDTCExtDataRecordByDTCNumber = 0x10,
    ReportNumberOfMirrorMemoryDTCByStatusMask = 0x11,
    ReportNumberOfEmissionsOBDDTCByStatusMask = 0x12,
    ReportEmissionsOBDDTCByStatusMask = 0x13,
    ReportDTCFaultDetectionCounter = 0x14,
    ReportDTCWithPermanentStatus = 0x15,
    ReportDTCExtDataRecordByRecordNumber = 0x16,
    ReportUserDefMemoryDTCByStatusMask = 0x17,
    ReportUserDefMemoryDTCSnapshotRecordByDTCNumber = 0x18,
    ReportUserDefMemoryDTCExtDataRecordByDTCNumber = 0x19,
    ReportSupportedDTCExtDataRecord = 0x1A,
    ReportWWHOBDDTCByMaskRecord = 0x42,
    ReportWWHOBDDTCWithPermanentStatus = 0x55,
    ReportDTCInformationByDTCReadinessGroupIdentifier = 0x56,
}

/// Authentication (0x29) `authenticationReturnParameter`.
#[repr(u8)]
#[derive(Debug, FromRepr)]
pub enum AuthenticationReturn {
    RequestAccepted = 0x00,
    GeneralReject = 0x01,
    AuthenticationConfigurationAPCE = 0x02,
    AuthenticationConfigurationACRWithAsymmetricCrypto = 0x03,
    AuthenticationConfigurationACRWithSymmetricCrypto = 0x04,
    DeAuthenticationSuccessful = 0x10,
    CertificateVerifiedOwnershipVerificationNecessary = 0x11,
    OwnershipVerifiedAuthenticationComplete = 0x12,
    CertificateVerified = 0x13,
}

/// Standardized data identifiers from ISO 14229-1 (identification range).
#[repr(u16)]
#[derive(Debug, FromRepr)]
pub enum DataIdentifier {
    BootSoftwareIdentification = 0xF180,
    ApplicationSoftwareIdentification = 0xF181,
    ApplicationDataIdentification = 0xF182,
    BootSoftwareFingerprint = 0xF183,
    ApplicationSoftwareFingerprint = 0xF184,
    ApplicationDataFingerprint = 0xF185,
    ActiveDiagnosticSession = 0xF186,
    VehicleManufacturerSparePartNumber = 0xF187,
    VehicleManufacturerECUSoftwareNumber = 0xF188,
    VehicleManufacturerECUSoftwareVersionNumber = 0xF189,
    SystemSupplierIdentifier = 0xF18A,
    ECUManufacturingDate = 0xF18B,
    ECUSerialNumber = 0xF18C,
    SupportedFunctionalUnits = 0xF18D,
    VehicleManufacturerKitAssemblyPartNumber = 0xF18E,
    VIN = 0xF190,
    VehicleManufacturerECUHardwareNumber = 0xF191,
    SystemSupplierECUHardwareNumber = 0xF192,
    SystemSupplierECUHardwareVersionNumber = 0xF193,
    SystemSupplierECUSoftwareNumber = 0xF194,
    SystemSupplierECUSoftwareVersionNumber = 0xF195,
    ExhaustRegulationOrTypeApprovalNumber = 0xF196,
    SystemNameOrEngineType = 0xF197,
    RepairShopCodeOrTesterSerialNumber = 0xF198,
    ProgrammingDate = 0xF199,
    CalibrationRepairShopCode = 0xF19A,
    CalibrationDate = 0xF19B,
    CalibrationEquipmentSoftwareNumber = 0xF19C,
    ECUInstallationDate = 0xF19D,
    ODXFile = 0xF19E,
    Entity = 0xF19F,
}

#[allow(nonstandard_style)]
pub struct UDS_DTC {
    pub category: DTCCategory,
    pub number: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct DtcStatus(pub u8);

/// Bit assignments of the ISO 14229 DTC status byte; also the bits of the
/// status mask sent in [`UDSService::ReadDTCInformation`] requests, where a
/// DTC matches if it has any requested bit set.
#[repr(u8)]
#[derive(Debug, Clone, Copy, VariantArray)]
pub enum DtcStatusBit {
    TestFailed = 0x01,
    TestFailedThisOperationCycle = 0x02,
    PendingDTC = 0x04,
    ConfirmedDTC = 0x08,
    TestNotCompletedSinceLastClear = 0x10,
    TestFailedSinceLastClear = 0x20,
    TestNotCompletedThisOperationCycle = 0x40,
    WarningIndicatorRequested = 0x80,
}

impl ISOTPService for UDSService {
    fn service_byte(&self) -> u8 {
        *self as u8
    }

    fn demux(&self, context: &DemuxContext) {
        let data = context.data;

        match self {
            UDSService::NegativeResponce => demux_negative_response(data),
            UDSService::DiagnosticSessionControl => demux_session_control(data),
            UDSService::ECUReset => demux_ecu_reset(data),
            UDSService::SecurityAccess => demux_security_access(data),
            UDSService::CommunicationControl => demux_communication_control(data),
            UDSService::TesterPresent => demux_tester_present(data),
            UDSService::ControlDTCSettings => demux_control_dtc_settings(data),
            UDSService::LinkControl => demux_link_control(data),
            UDSService::Authentication => demux_authentication(data),
            UDSService::ReadDataByIdentifier => demux_read_data_by_identifier(data),
            UDSService::WriteDataByIdentifier => demux_write_data_by_identifier(data),
            UDSService::DynamicallyDefineDataIdentifier => demux_dynamically_define(data),
            UDSService::InputOutputControlByIdentifier => demux_io_control(data),
            UDSService::RoutineControl => demux_routine_control(data),
            UDSService::ReadDTCInformation => demux_read_dtc_information(data),
            UDSService::ClearDiagnosticInformation => demux_clear_diagnostic_information(data),
            UDSService::RequestDownload => demux_request_transfer(data, "RequestDownload"),
            UDSService::RequestUpload => demux_request_transfer(data, "RequestUpload"),
            UDSService::TransferData => demux_transfer_data(data),
            UDSService::RequestTransferExit => demux_request_transfer_exit(data),
            other => {
                let sid = other.clone() as u8 + SERVICE_OFFSET;
                println!("Unimplemented UDS service {other:?} ({sid:#02X}); data: {data:#02X?}");
            }
        }
    }
}

fn demux_negative_response(data: &[u8]) {
    match data {
        [] => println!("UDS negative response with no payload"),
        [sid] => println!(
            "UDS negative response for service {} with no NRC",
            service_name(*sid)
        ),
        [sid, nrc, ..] => {
            let service = service_name(*sid);
            if let Some(code) = UDSNegativeResponce::from_repr(*nrc) {
                println!("UDS negative response to {service}: {code:?} ({nrc:#02X})");
            } else {
                println!("UDS negative response to {service}: unknown NRC {nrc:#02X}");
            }
        }
    }
}

fn demux_session_control(data: &[u8]) {
    match data {
        [] => println!("UDS DiagnosticSessionControl response with no sub-function"),
        [session, p2_hi, p2_lo, p2star_hi, p2star_lo, ..] => {
            let p2 = u16::from_be_bytes([*p2_hi, *p2_lo]);
            // P2*server_max is reported in units of 10 ms.
            let p2_star = u32::from(u16::from_be_bytes([*p2star_hi, *p2star_lo])) * 10;
            println!(
                "UDS DiagnosticSessionControl -> {}; P2_server_max={p2} ms, P2*_server_max={p2_star} ms",
                session_name(*session)
            );
        }
        [session, ..] => {
            println!("UDS DiagnosticSessionControl -> {}", session_name(*session));
        }
    }
}

fn demux_ecu_reset(data: &[u8]) {
    match data {
        [] => println!("UDS ECUReset response with no reset type"),
        [reset, rest @ ..] => {
            let name = ECUResetType::from_repr(reset & !SUPPRESS_POS_RSP_BIT)
                .map(|r| format!("{r:?}"))
                .unwrap_or_else(|| format!("{reset:#02X}"));

            if let [power_down] = rest {
                println!("UDS ECUReset -> {name}; powerDownTime={power_down} s");
            } else {
                println!("UDS ECUReset -> {name}");
            }
        }
    }
}

fn demux_security_access(data: &[u8]) {
    match data {
        [] => println!("UDS SecurityAccess response with no access type"),
        [access, seed @ ..] => {
            let level = access & !SUPPRESS_POS_RSP_BIT;
            // Odd access types request a seed; even ones deliver a key.
            if level % 2 == 1 {
                if seed.iter().all(|byte| *byte == 0) {
                    println!("UDS SecurityAccess level {level:#02X}: already unlocked (zero seed)");
                } else {
                    println!("UDS SecurityAccess level {level:#02X}: seed {seed:#02X?}");
                }
            } else {
                println!("UDS SecurityAccess level {level:#02X}: key accepted");
            }
        }
    }
}

fn demux_communication_control(data: &[u8]) {
    match data.first() {
        None => println!("UDS CommunicationControl response with no control type"),
        Some(control) => {
            let name = CommunicationControlType::from_repr(control & !SUPPRESS_POS_RSP_BIT)
                .map(|c| format!("{c:?}"))
                .unwrap_or_else(|| format!("{control:#02X}"));
            println!("UDS CommunicationControl -> {name}");
        }
    }
}

fn demux_tester_present(_data: &[u8]) {
    println!("UDS TesterPresent acknowledged");
}

fn demux_control_dtc_settings(data: &[u8]) {
    match data.first() {
        None => println!("UDS ControlDTCSettings response with no setting type"),
        Some(setting) => {
            let name = DTCSettingType::from_repr(setting & !SUPPRESS_POS_RSP_BIT)
                .map(|s| format!("{s:?}"))
                .unwrap_or_else(|| format!("{setting:#02X}"));
            println!("UDS ControlDTCSettings -> {name}");
        }
    }
}

fn demux_link_control(data: &[u8]) {
    match data.first() {
        None => println!("UDS LinkControl response with no sub-function"),
        Some(sub) => println!(
            "UDS LinkControl sub-function {:#02X}",
            sub & !SUPPRESS_POS_RSP_BIT
        ),
    }
}

fn demux_authentication(data: &[u8]) {
    match data {
        [task, ret, rest @ ..] => {
            let task = task & !SUPPRESS_POS_RSP_BIT;
            let ret_name = AuthenticationReturn::from_repr(*ret)
                .map(|r| format!("{r:?}"))
                .unwrap_or_else(|| format!("{ret:#02X}"));

            if rest.is_empty() {
                println!("UDS Authentication task {task:#02X}: {ret_name}");
            } else {
                println!("UDS Authentication task {task:#02X}: {ret_name}; {rest:#02X?}");
            }
        }
        _ => println!("UDS Authentication malformed: {data:#02X?}"),
    }
}

fn demux_read_data_by_identifier(data: &[u8]) {
    match split_did(data) {
        Some((did, value)) => {
            println!(
                "UDS ReadDataByIdentifier {}: {}",
                did_name(did),
                render_bytes(value)
            );
        }
        None => println!("UDS ReadDataByIdentifier malformed: {data:#02X?}"),
    }
}

fn demux_write_data_by_identifier(data: &[u8]) {
    match split_did(data) {
        Some((did, _)) => println!("UDS WriteDataByIdentifier {}: written", did_name(did)),
        None => println!("UDS WriteDataByIdentifier malformed: {data:#02X?}"),
    }
}

fn demux_dynamically_define(data: &[u8]) {
    match data {
        [] => println!("UDS DynamicallyDefineDataIdentifier response with no definition type"),
        [definition, rest @ ..] => {
            let name = DynamicDefinitionType::from_repr(definition & !SUPPRESS_POS_RSP_BIT)
                .map(|d| format!("{d:?}"))
                .unwrap_or_else(|| format!("{definition:#02X}"));

            if let [did_hi, did_lo, ..] = rest {
                let did = u16::from_be_bytes([*did_hi, *did_lo]);
                println!("UDS DynamicallyDefineDataIdentifier {name}: dynamic DID {did:#04X}");
            } else {
                println!("UDS DynamicallyDefineDataIdentifier {name}");
            }
        }
    }
}

fn demux_io_control(data: &[u8]) {
    match split_did(data) {
        Some((did, record)) => println!(
            "UDS InputOutputControlByIdentifier {}: controlStatusRecord {record:#02X?}",
            did_name(did)
        ),
        None => println!("UDS InputOutputControlByIdentifier malformed: {data:#02X?}"),
    }
}

fn demux_routine_control(data: &[u8]) {
    match data {
        [control, rid_hi, rid_lo, record @ ..] => {
            let name = RoutineControlType::from_repr(control & !SUPPRESS_POS_RSP_BIT)
                .map(|c| format!("{c:?}"))
                .unwrap_or_else(|| format!("{control:#02X}"));
            let rid = u16::from_be_bytes([*rid_hi, *rid_lo]);

            if record.is_empty() {
                println!("UDS RoutineControl {name} routine {rid:#04X}");
            } else {
                println!(
                    "UDS RoutineControl {name} routine {rid:#04X}; statusRecord {record:#02X?}"
                );
            }
        }
        _ => println!("UDS RoutineControl malformed: {data:#02X?}"),
    }
}

fn demux_read_dtc_information(data: &[u8]) {
    let Some((report, rest)) = data.split_first() else {
        println!("UDS ReadDTCInformation response with no report type");
        return;
    };

    let report_type = DTCReportType::from_repr(*report);
    let name = report_type
        .as_ref()
        .map(|r| format!("{r:?}"))
        .unwrap_or_else(|| format!("{report:#02X}"));

    match report_type {
        Some(
            DTCReportType::ReportNumberOfDTCByStatusMask
            | DTCReportType::ReportNumberOfDTCBySeverityMaskRecord
            | DTCReportType::ReportNumberOfMirrorMemoryDTCByStatusMask
            | DTCReportType::ReportNumberOfEmissionsOBDDTCByStatusMask,
        ) => {
            if let [avail, format, hi, lo] = rest {
                let count = u16::from_be_bytes([*hi, *lo]);
                println!(
                    "UDS ReadDTCInformation {name}: {count} DTC(s); availabilityMask={}, formatId={format:#02X}",
                    DtcStatus(*avail)
                );
            } else {
                println!("UDS ReadDTCInformation {name}: {rest:#02X?}");
            }
        }
        Some(
            DTCReportType::ReportDTCByStatusMask
            | DTCReportType::ReportSupportedDTC
            | DTCReportType::ReportDTCWithPermanentStatus
            | DTCReportType::ReportMirrorMemoryDTCByStatusMask
            | DTCReportType::ReportEmissionsOBDDTCByStatusMask
            | DTCReportType::ReportUserDefMemoryDTCByStatusMask
            | DTCReportType::ReportFirstTestFailedDTC
            | DTCReportType::ReportFirstConfirmedDTC
            | DTCReportType::ReportMostRecentTestFailedDTC
            | DTCReportType::ReportMostRecentConfirmedDTC,
        ) => {
            let Some((avail, records)) = rest.split_first() else {
                println!("UDS ReadDTCInformation {name}: no availability mask");
                return;
            };

            println!(
                "UDS ReadDTCInformation {name}: availabilityMask={}",
                DtcStatus(*avail)
            );

            let (records, remainder) = records.as_chunks::<{ UDS_DTC::SIZE + 1 }>();
            for record in records {
                let dtc = UDS_DTC::from([record[0], record[1], record[2]]);
                println!("  {dtc} status={}", DtcStatus(record[3]));
            }
            if !remainder.is_empty() {
                println!("  trailing bytes: {remainder:#02X?}");
            }
        }
        Some(
            DTCReportType::ReportDTCBySeverityMaskRecord
            | DTCReportType::ReportSeverityInformationOfDTC,
        ) => {
            let Some((avail, records)) = rest.split_first() else {
                println!("UDS ReadDTCInformation {name}: no availability mask");
                return;
            };

            println!(
                "UDS ReadDTCInformation {name}: availabilityMask={}",
                DtcStatus(*avail)
            );

            let (records, remainder) = records.as_chunks::<{ UDS_DTC::SIZE + 3 }>();
            for record in records {
                let dtc = UDS_DTC::from([record[2], record[3], record[4]]);
                println!(
                    "  {dtc} severity={:#02X} functionalUnit={:#02X} status={}",
                    record[0],
                    record[1],
                    DtcStatus(record[5])
                );
            }
            if !remainder.is_empty() {
                println!("  trailing bytes: {remainder:#02X?}");
            }
        }
        Some(DTCReportType::ReportDTCSnapshotIdentification) => {
            println!("UDS ReadDTCInformation {name}:");

            // DTCRecord: DTC(3) snapshotRecordNumber(1)
            let (records, remainder) = rest.as_chunks::<{ UDS_DTC::SIZE + 1 }>();
            for record in records {
                let dtc = UDS_DTC::from([record[0], record[1], record[2]]);
                println!("  {dtc} snapshotRecord={:#02X}", record[3]);
            }
            if !remainder.is_empty() {
                println!("  trailing bytes: {remainder:#02X?}");
            }
        }
        Some(DTCReportType::ReportDTCFaultDetectionCounter) => {
            println!("UDS ReadDTCInformation {name}:");

            // DTCFaultDetectionCounterRecord: DTC(3) counter(1)
            let (records, remainder) = rest.as_chunks::<{ UDS_DTC::SIZE + 1 }>();
            for record in records {
                let dtc = UDS_DTC::from([record[0], record[1], record[2]]);
                println!("  {dtc} faultDetectionCounter={}", record[3]);
            }
            if !remainder.is_empty() {
                println!("  trailing bytes: {remainder:#02X?}");
            }
        }
        _ => println!("UDS ReadDTCInformation {name}: {rest:#02X?}"),
    }
}

fn demux_clear_diagnostic_information(data: &[u8]) {
    if data.is_empty() {
        println!("UDS ClearDiagnosticInformation acknowledged");
    } else {
        println!("UDS ClearDiagnosticInformation acknowledged; unexpected data {data:#02X?}");
    }
}

fn demux_request_transfer(data: &[u8], label: &str) {
    match data {
        [] => println!("UDS {label} response with no lengthFormatIdentifier"),
        [format, rest @ ..] => {
            let len_bytes = (format >> 4) as usize;
            let block = rest.get(..len_bytes).unwrap_or(rest);
            let max_block = block
                .iter()
                .fold(0u64, |acc, byte| (acc << 8) | u64::from(*byte));
            println!("UDS {label}: maxNumberOfBlockLength={max_block} ({block:#02X?})");
        }
    }
}

fn demux_transfer_data(data: &[u8]) {
    match data {
        [] => println!("UDS TransferData response with no block sequence counter"),
        [block, rest @ ..] => {
            println!(
                "UDS TransferData block {block}; {} byte(s): {rest:#02X?}",
                rest.len()
            );
        }
    }
}

fn demux_request_transfer_exit(data: &[u8]) {
    if data.is_empty() {
        println!("UDS RequestTransferExit acknowledged");
    } else {
        println!("UDS RequestTransferExit acknowledged; parameterRecord {data:#02X?}");
    }
}

fn split_did(data: &[u8]) -> Option<(u16, &[u8])> {
    if let [hi, lo, rest @ ..] = data {
        Some((u16::from_be_bytes([*hi, *lo]), rest))
    } else {
        None
    }
}

fn did_name(did: u16) -> String {
    DataIdentifier::from_repr(did)
        .map(|identifier| format!("{identifier:?} ({did:#04X})"))
        .unwrap_or_else(|| format!("DID {did:#04X}"))
}

fn session_name(session: u8) -> String {
    DiagnosticSessionControlType::from_repr(session & !SUPPRESS_POS_RSP_BIT)
        .map(|s| format!("{s:?}"))
        .unwrap_or_else(|| format!("{session:#02X}"))
}

fn service_name(sid: u8) -> String {
    UDSService::from_repr(sid)
        .map(|service| format!("{service:?}"))
        .unwrap_or_else(|| format!("{sid:#02X}"))
}

fn render_bytes(data: &[u8]) -> String {
    if data.is_empty() {
        "<empty>".to_string()
    } else if data
        .iter()
        .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
    {
        format!("\"{}\" ({data:#02X?})", String::from_utf8_lossy(data))
    } else {
        format!("{data:#02X?}")
    }
}

impl UDS_DTC {
    pub const SIZE: usize = 3;
}

impl From<[u8; UDS_DTC::SIZE]> for UDS_DTC {
    fn from(value: [u8; UDS_DTC::SIZE]) -> Self {
        let category = value[0] >> 6;

        let mut number = (value[0] & 0x3F) as u32;
        for byte in &value[1..] {
            number = (number << 8) | *byte as u32;
        }

        Self {
            category: DTCCategory::from_repr(category).expect("infallible"),
            number,
        }
    }
}

impl std::fmt::Display for UDS_DTC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{:04X}-{:02X}",
            Into::<char>::into(self.category).to_uppercase(),
            self.number >> 8,
            self.number & 0xFF,
        )
    }
}

impl DtcStatus {
    /// Status mask matching a DTC in any state
    pub const ANY: Self = Self(
        DtcStatusBit::TestFailed as u8
            | DtcStatusBit::TestFailedThisOperationCycle as u8
            | DtcStatusBit::PendingDTC as u8
            | DtcStatusBit::ConfirmedDTC as u8
            | DtcStatusBit::TestNotCompletedSinceLastClear as u8
            | DtcStatusBit::TestFailedSinceLastClear as u8
            | DtcStatusBit::TestNotCompletedThisOperationCycle as u8
            | DtcStatusBit::WarningIndicatorRequested as u8,
    );

    pub fn contains(&self, bit: DtcStatusBit) -> bool {
        self.0 & (bit as u8) != 0
    }
}

impl From<DtcStatus> for u8 {
    fn from(status: DtcStatus) -> u8 {
        status.0
    }
}

impl std::fmt::Display for DtcStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let set: Vec<String> = DtcStatusBit::VARIANTS
            .iter()
            .filter(|bit| self.contains(**bit))
            .map(|bit| format!("{bit:?}"))
            .collect();

        if set.is_empty() {
            write!(f, "{:#02X} (none)", self.0)
        } else {
            write!(f, "{:#02X} [{}]", self.0, set.join(", "))
        }
    }
}
