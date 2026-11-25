# Hardware discovery

## Purpose

Defines how nicecooling finds Boards, sensors and Channels, names them, computes Derived sensors and flow values, and groups them into Loops.

## ADDED Requirements

### Requirement: Discover Boards, sensors and Channels
The daemon SHALL discover the sensors and Channels of every Board that the kernel exposes through hwmon, without a fixed list of supported Boards.

#### Scenario: Reference Board discovered
- **WHEN** the daemon starts on the reference Board
- **THEN** it lists the 7 Channels of the Super I/O chip and the sensors of the embedded controller

#### Scenario: Unknown Board discovered
- **WHEN** the daemon starts on a Board that has no Board file
- **THEN** it lists the Channels and sensors with the names that the kernel gives them

#### Scenario: Other Aqua Computer device with a kernel driver
- **WHEN** a kernel driver exposes the sensors of an Aqua Computer device other than the Leakshield through hwmon
- **THEN** the daemon lists those sensors through generic discovery and offers no write support for the device

### Requirement: Sensor origins
The daemon SHALL give each sensor one origin: measured, Derived or Graph sensor. The app computes a Graph sensor and sends its value to the daemon, so the daemon has no source task for it.

#### Scenario: Graph sensor in the list
- **WHEN** a client lists the sensors and a Graph sensor exists
- **THEN** the list holds it with the origin `graph`, next to the measured and Derived sensors

### Requirement: Stable device identity
The daemon SHALL identify a sensor or Channel by the hwmon `name` plus the resolved path of the hwmon `device` link, and MUST NOT identify it by the hwmon index. It SHALL take the driver for exclusion lists from the name of the `device/driver` link. It SHALL identify a Board by the DMI values `board_vendor` and `board_name`.

#### Scenario: Index changes between boots
- **WHEN** the hwmon index of a driver differs from the previous boot
- **THEN** every Curve, Loop and name in the configuration still refers to the same Channel or sensor

### Requirement: GPU as an Input
The daemon SHALL offer the GPU sensors that the hwmon driver exposes as Inputs. It MUST NOT control the GPU fan. It SHALL take Channels only from hwmon devices whose driver is not `amdgpu`, `radeon`, `nouveau`, `i915`, `xe` or `aquacomputer_d5next`.

#### Scenario: GPU temperature in a Curve
- **WHEN** the user selects a GPU temperature as the Input of a Curve
- **THEN** the Curve follows that temperature

#### Scenario: GPU fan not a Channel
- **WHEN** the `amdgpu` hwmon device has `pwm1` and `pwm1_enable`
- **THEN** the daemon lists no Channel for that device and keeps its sensors as Inputs

### Requirement: One source for the Leakshield
The daemon SHALL NOT list the kernel hwmon device `leakshield`. The daemon SHALL decode report 1 itself.

#### Scenario: Kernel driver loaded
- **WHEN** the kernel driver `aquacomputer_d5next` exposes the hwmon device `leakshield`
- **THEN** discovery lists no sensor or Channel from that device, and the Leakshield sensors come from the daemon

### Requirement: Board files and user names
The daemon SHALL use the names of the Board file for the Channels and sensors of a known Board. The Board file SHALL be TOML data shipped in the RPM in `/usr/share/nicecooling/boards/`. The daemon SHALL select the Board file whose `board_vendor` and `board_name` equal the DMI values. A user name SHALL replace the Board file name and SHALL be stored in the daemon state.

#### Scenario: Board file name
- **WHEN** the Board is the reference Board and the user set no name
- **THEN** Channel 6 has the name "Waterpump"

#### Scenario: User name wins
- **WHEN** the user renames Channel 6 to "Loop pump"
- **THEN** every client shows "Loop pump" and the Board file is unchanged

### Requirement: Derived sensors
The daemon SHALL compute Derived sensors from other sensors. It SHALL support a maximum, a mean and a difference, and a Curve SHALL accept a Derived sensor as Input.

#### Scenario: Maximum of two sensors
- **WHEN** a Derived sensor is the maximum of two sensors that read 40 and 55
- **THEN** the Derived sensor reads 55

#### Scenario: Missing source sensor
- **WHEN** a source sensor of a Derived sensor has no reading
- **THEN** the Derived sensor has no reading and a Curve that uses it reports the fault

### Requirement: Flow Calibration table
The daemon SHALL convert the raw pulse rate of a flow sensor to a flow in L/h. It SHALL use the Calibration table of that sensor and interpolate linearly between its points.

#### Scenario: Reading between two points
- **WHEN** the rate lies between two points of the table
- **THEN** the flow lies on the straight line between those two points

#### Scenario: Reading below the first point
- **WHEN** the rate is below the first point
- **THEN** the flow follows the line from 0 rpm = 0 L/h to the first point

#### Scenario: Reading above the last point
- **WHEN** the rate is above the last point
- **THEN** the flow follows the extended last segment and the value carries an out-of-range flag

### Requirement: Loops
The daemon SHALL let the user define a Loop with its Pump channels, its flow sensor and Calibration table, its water temperature sensor and, optionally, its Leakshield.

#### Scenario: Loop without Leakshield
- **WHEN** the user defines a Loop with no Leakshield
- **THEN** the Loop works with its Pump floor and its pump-stop Alarm, and no Feed is sent

#### Scenario: No air temperature probe
- **WHEN** a Loop has no air temperature sensor
- **THEN** the daemon offers no water-to-air delta T for that Loop
