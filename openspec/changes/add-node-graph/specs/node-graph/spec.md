# Node graph

## Purpose

Defines the Graph: a set of nodes and connections that the daemon evaluates once per second and that ends in one sensor node or one action node. It covers the no-value rule, the arithmetic of every node kind, the evaluation order, the loop checks, storage, export and import, the conversion of the Derived sensors, the D-Bus objects and the Graph editor of the app.

## ADDED Requirements

### Requirement: Graph model
A Graph SHALL have an id, a name, an enabled flag, a list of nodes and a list of connections. A node SHALL have an id, a kind, parameters, an optional label and a canvas position. A connection SHALL join one output pin to one input pin. An input pin SHALL take at most one connection. An output pin SHALL take any number.

#### Scenario: Two connections on one input pin
- **WHEN** a Graph write holds two connections that end on the same input pin
- **THEN** the daemon refuses the write with `InvalidArgument` and keeps the stored Graph

#### Scenario: One output feeds two nodes
- **WHEN** one output pin has connections to two input pins of two nodes
- **THEN** the daemon accepts the Graph and both nodes read the same value

### Requirement: One end node
A Graph SHALL have exactly one end node: a sensor node or an action node. The end node SHALL have no output connection. The daemon SHALL refuse a Graph with no end node or with two.

#### Scenario: Graph without an end node
- **WHEN** a Graph write holds a constant node and an addition node and no end node
- **THEN** the daemon refuses the write with `InvalidArgument`

#### Scenario: Graph with two end nodes
- **WHEN** a Graph write holds a sensor node and an action node
- **THEN** the daemon refuses the write with `InvalidArgument`

#### Scenario: Connection from an end node
- **WHEN** a Graph write holds a connection that starts on the pin of the end node
- **THEN** the daemon refuses the write with `InvalidArgument`

### Requirement: Graph limits
The daemon SHALL store at most 64 Graphs and at most 256 nodes in one Graph. A Graph name and a node label SHALL have at most 64 characters. A write above a limit SHALL fail with `InvalidArgument`.

#### Scenario: 65th Graph
- **WHEN** 64 Graphs exist and the user applies a new Graph
- **THEN** the daemon refuses it with `InvalidArgument`

#### Scenario: 257 nodes
- **WHEN** a Graph write holds 257 nodes
- **THEN** the daemon refuses it with `InvalidArgument`

#### Scenario: Name of 65 characters
- **WHEN** a Graph write has a name of 65 characters
- **THEN** the daemon refuses it with `InvalidArgument`

### Requirement: No-value rule
A pin SHALL carry a number or no value. A node with a connected input pin that has no value SHALL give no value and SHALL keep its state. This SHALL hold for every kind. A division by 0, a function outside its domain and a result that is not finite SHALL give no value.

#### Scenario: Missing input
- **WHEN** an addition node has input A connected to a source with no value and input B connected to the constant 5
- **THEN** the addition node gives no value and the node after it gives no value

#### Scenario: State survives a gap
- **WHEN** a memory node holds 7 and its input S loses its value for 3 ticks and then returns with the value 0
- **THEN** the node gives no value for 3 ticks and then gives 7

#### Scenario: Division by zero
- **WHEN** a division node has A = 4 and B = 0
- **THEN** the node gives no value

### Requirement: Logic values
A logic output SHALL be 1 or 0. A logic input SHALL be true when its value is above 0 and false when it is 0 or below.

#### Scenario: Negative input
- **WHEN** an and node has A = -1 and B = 1
- **THEN** the node gives 0

#### Scenario: Small positive input
- **WHEN** an or node has A = 0.001 and B = 0
- **THEN** the node gives 1

### Requirement: Unconnected input pins
An input pin without a connection SHALL take the default of its kind: 0, except the base of the logarithm (the number e), the exponent of the power (2), and the enable pin (1) and the period pin (10 s) of the generators and the counter. An unconnected pin SHALL count as drawn, not as missing.

#### Scenario: Unconnected base
- **WHEN** a logarithm node has A connected to the constant 1 and B unconnected
- **THEN** the node gives 0, the natural logarithm of 1

#### Scenario: Unconnected exponent
- **WHEN** a power node has A connected to the constant 3 and B unconnected
- **THEN** the node gives 9

#### Scenario: Unconnected addition input
- **WHEN** an addition node has A connected to the constant 5 and B unconnected
- **THEN** the node gives 5

### Requirement: Sensor source node
The sensor source node SHALL give the current value of the chosen sensor, rounded to 8 decimals. It SHALL give no value while the sensor has no value. It SHALL accept any sensor, a Graph sensor included.

#### Scenario: Live sensor
- **WHEN** the chosen sensor reads 46.123456789
- **THEN** the node gives 46.12345679

#### Scenario: Stale sensor
- **WHEN** the chosen sensor has had no reading for more than 3 seconds
- **THEN** the node gives no value

#### Scenario: Sensor that does not exist
- **WHEN** the chosen sensor id is not a sensor of this machine
- **THEN** the node is unbound and gives no value

### Requirement: Alarm event state source node
The alarm event state source node SHALL have an event id and a source id. It SHALL give 1 while an Alarm or Warning instance of that event is active for that source, and 0 otherwise. It SHALL read the state of the same tick.

#### Scenario: Pump-stop Alarm active
- **WHEN** the node is set to `pump-stop` of `loop_1` and that Alarm is active
- **THEN** the node gives 1

#### Scenario: Other Loop
- **WHEN** the node is set to `pump-stop` of `loop_1` and the Alarm is active only for `loop_2`
- **THEN** the node gives 0

#### Scenario: Alarm closes
- **WHEN** the Alarm closes in a tick
- **THEN** the node gives 0 in that tick

### Requirement: Constant and random nodes
The constant node SHALL give its value, a finite number with default 0. The random node SHALL give a new number in its range each tick, rounded to 3 decimals. The range SHALL have a minimum and a maximum with minimum not above maximum. The default range SHALL be -100 to 100.

#### Scenario: Constant
- **WHEN** a constant node has the value 2.5
- **THEN** it gives 2.5 in every tick

#### Scenario: Random range
- **WHEN** a random node has the range 10 to 20 and runs for 1000 ticks
- **THEN** every value is between 10 and 20 and has at most 3 decimals

#### Scenario: Inverted range
- **WHEN** a Graph write has a random node with minimum 5 and maximum 1
- **THEN** the daemon refuses the write with `InvalidArgument`

### Requirement: Process node
The process node SHALL give 1 while a process with the chosen name runs, and 0 otherwise. It SHALL use the process scan and the name match of the Profile process rule. The name SHALL have 1 to 255 characters.

#### Scenario: Process runs
- **WHEN** the node has the name `blender` and a process with the `comm` value `Blender` runs
- **THEN** the node gives 1

#### Scenario: Process ends
- **WHEN** the last process of that name ends
- **THEN** the node gives 0 in the next tick

### Requirement: Arithmetic of two inputs
The nodes addition, subtraction, multiplication, division, minimum, maximum and average SHALL give A + B, A - B, A x B, A / B, the smaller of A and B, the larger of A and B, and (A + B) / 2. Division SHALL give no value when B is 0.

#### Scenario: Subtraction
- **WHEN** a subtraction node has A = 10 and B = 4
- **THEN** the node gives 6

#### Scenario: Average
- **WHEN** an average node has A = 10 and B = 4
- **THEN** the node gives 7

#### Scenario: Minimum and maximum
- **WHEN** A = -2 and B = 3
- **THEN** the minimum node gives -2 and the maximum node gives 3

#### Scenario: Division
- **WHEN** a division node has A = 9 and B = 4
- **THEN** the node gives 2.25

### Requirement: Logarithm and power nodes
The logarithm node SHALL give the logarithm of A to the base B. It SHALL give no value when A is 0 or below, when B is 0 or below, or when B is 1. The power node SHALL give A to the power B and SHALL give no value when the result is not finite.

#### Scenario: Logarithm base 10
- **WHEN** a logarithm node has A = 1000 and B = 10
- **THEN** the node gives 3

#### Scenario: Logarithm of zero
- **WHEN** a logarithm node has A = 0
- **THEN** the node gives no value

#### Scenario: Power with a fractional exponent
- **WHEN** a power node has A = -8 and B = 0.5
- **THEN** the node gives no value

#### Scenario: Power
- **WHEN** a power node has A = 2 and B = 10
- **THEN** the node gives 1024

### Requirement: Trigonometry and square root nodes
The sine, cosine and tangent nodes SHALL take A in radians. The square root node SHALL give the square root of A and SHALL give no value when A is below 0.

#### Scenario: Sine
- **WHEN** a sine node has A = 1.5707963267948966
- **THEN** the node gives 1

#### Scenario: Square root of a negative number
- **WHEN** a square root node has A = -4
- **THEN** the node gives no value

#### Scenario: Square root
- **WHEN** a square root node has A = 16
- **THEN** the node gives 4

### Requirement: Round node
The round node SHALL round A to 0 to 8 decimals in one of three modes: to the nearest, down or up. The nearest mode SHALL round a tie away from 0. The defaults SHALL be 0 decimals and the nearest mode.

#### Scenario: Nearest
- **WHEN** a round node has 1 decimal, the nearest mode and A = 2.25
- **THEN** the node gives 2.3

#### Scenario: Down
- **WHEN** a round node has 0 decimals, the down mode and A = -1.5
- **THEN** the node gives -2

#### Scenario: Up
- **WHEN** a round node has 2 decimals, the up mode and A = 3.141
- **THEN** the node gives 3.15

### Requirement: Limiter node
The limiter node SHALL hold A between a minimum and a maximum. The defaults SHALL be -100 and 100. The daemon SHALL refuse a minimum above the maximum.

#### Scenario: Above the maximum
- **WHEN** a limiter node has the range 0 to 80 and A = 95
- **THEN** the node gives 80

#### Scenario: Inside the range
- **WHEN** a limiter node has the range 0 to 80 and A = 42
- **THEN** the node gives 42

### Requirement: Table node
The table node SHALL map A through 2 to 64 pairs of an input and an output. It SHALL sort the pairs by input and SHALL refuse two pairs with the same input. The mode SHALL be linear or step. A at or below the first input SHALL give the first output. A at or above the last input SHALL give the last output. The default pairs SHALL be (-100, -100) and (100, 100).

#### Scenario: Linear between two pairs
- **WHEN** the pairs are (20, 30) and (40, 80), the mode is linear and A = 30
- **THEN** the node gives 55

#### Scenario: Step mode
- **WHEN** the pairs are (20, 30) and (40, 80), the mode is step and A = 39.9
- **THEN** the node gives 30

#### Scenario: Above the last input
- **WHEN** the pairs are (20, 30) and (40, 80) and A = 90
- **THEN** the node gives 80

#### Scenario: Same input twice
- **WHEN** a Graph write has two pairs with the input 20
- **THEN** the daemon refuses the write with `InvalidArgument`

### Requirement: Comparison nodes
The nodes less, less or equal, equal, greater or equal, greater and not equal SHALL compare A with B as exact numbers and SHALL give 1 when the comparison is true and 0 when it is false. A comparison SHALL use no tolerance.

#### Scenario: Greater
- **WHEN** a greater node has A = 40.1 and B = 40
- **THEN** the node gives 1

#### Scenario: Equal without tolerance
- **WHEN** an equal node has A = 0.1 + 0.2 as computed by a node and B = 0.3
- **THEN** the node gives 0

#### Scenario: Not equal
- **WHEN** a not equal node has A = 5 and B = 5
- **THEN** the node gives 0

### Requirement: Logic nodes
The and node SHALL give 1 when A and B are true. The or node SHALL give 1 when A or B is true. The not node SHALL give 1 when A is false. The switch node SHALL give A when S is false and B when S is true.

#### Scenario: And and or
- **WHEN** A = 1 and B = 0
- **THEN** the and node gives 0 and the or node gives 1

#### Scenario: Not
- **WHEN** a not node has A = 0
- **THEN** the node gives 1

#### Scenario: Switch
- **WHEN** a switch node has S = 1, A = 10 and B = 20
- **THEN** the node gives 20

#### Scenario: Switch with an input without value
- **WHEN** a switch node has S = 0, A = 10 and B connected to a source with no value
- **THEN** the node gives no value

### Requirement: Memory node
The memory node SHALL take A while S is true and SHALL hold the last taken value while S is false. Its start value SHALL be 0. It SHALL start again at 0 after a restart of the daemon.

#### Scenario: Take and hold
- **WHEN** S is 1 with A = 5, then S is 0 with A = 9
- **THEN** the node gives 5 in both ticks

#### Scenario: Restart
- **WHEN** the daemon restarts after the node held 5
- **THEN** the node gives 0 until S is true

### Requirement: Persistent storage node
The persistent storage node SHALL work as the memory node. It SHALL also keep its value over a restart of the daemon.

#### Scenario: Restart
- **WHEN** the daemon restarts after the node held 5 and S stays 0
- **THEN** the node gives 5

#### Scenario: Node removed
- **WHEN** the user applies the Graph without the node
- **THEN** the daemon deletes the stored value of the node

### Requirement: Trigger node
The trigger node SHALL give 1 for one tick when A goes from false to true, and 0 otherwise. The first tick with a value SHALL give 0 and SHALL record A.

#### Scenario: Rising edge
- **WHEN** A is 0 for 2 ticks, then 1 for 3 ticks
- **THEN** the node gives 0, 0, 1, 0, 0

#### Scenario: True at the start
- **WHEN** A is 1 in the first tick with a value
- **THEN** the node gives 0 in that tick

### Requirement: On delay and off delay nodes
The on delay node SHALL give 1 when A has been true for at least the delay, and 0 otherwise. The off delay node SHALL give 1 while A is true and for the delay after A becomes false. The delay SHALL be 0 to 3600 s, default 1 s. Both nodes SHALL start at 0.

#### Scenario: On delay
- **WHEN** the delay is 3 s and A becomes 1 at second 10 and stays 1
- **THEN** the node gives 0 at seconds 10, 11 and 12 and gives 1 from second 13

#### Scenario: On delay reset
- **WHEN** A falls to 0 at second 12 and rises again at second 14
- **THEN** the node gives 0 at second 12 and the 3 s start again at second 14

#### Scenario: Off delay
- **WHEN** the delay is 3 s, A is 1 until second 10 and 0 from second 11
- **THEN** the node gives 1 until second 13 and 0 from second 14

### Requirement: Counter node
The counter node SHALL have the inputs count, enable, reset and time. While enable is true it SHALL add count to its output once each time seconds. Reset true SHALL set the output and the timer to 0. The output SHALL start at 0. It SHALL add at most once per tick.

#### Scenario: Count every 5 seconds
- **WHEN** count is 2, time is 5 s and enable stays 1 for 12 s
- **THEN** the output is 0 at second 0, 2 at second 5 and 4 at second 10

#### Scenario: Reset
- **WHEN** reset is 1 in a tick
- **THEN** the output is 0 in that tick

#### Scenario: Disabled
- **WHEN** enable is 0
- **THEN** the output holds and the timer does not advance

### Requirement: Window filter nodes
The peak, minimum and average filter nodes SHALL give the largest, the smallest and the mean of the samples of a time window, rounded to 5 decimals. The span SHALL be 1 to 1800 s, default 10 s. Each tick SHALL add one sample, and then remove one sample when the window holds 3 or more and the oldest is older than the span.

#### Scenario: Window shrinks by one sample
- **WHEN** the span is 3 s and the values 1, 2, 3, 4, 5 arrive at seconds 0 to 4
- **THEN** the average node gives 3.5 at second 4 because the sample 1 left the window

#### Scenario: Peak
- **WHEN** the span is 10 s and the values 4, 9, 6 arrive
- **THEN** the peak node gives 9 after the third value

#### Scenario: No value in the window
- **WHEN** A has no value in a tick
- **THEN** the node gives no value and adds no sample

### Requirement: Low pass node
The low pass node SHALL use the window of the filter nodes. With fewer than 3 samples it SHALL give their mean. Otherwise it SHALL sort the samples, drop the lowest and the highest in pairs while at least a third of the samples would remain, take the mean of the rest, and move its output toward that mean by the factor 1 - damping / 100. Damping SHALL be 0 to 99.99, default 25.

#### Scenario: First samples
- **WHEN** the window holds the samples 10 and 20
- **THEN** the node gives 15

#### Scenario: Damped step
- **WHEN** the window holds 10, 20 and 30, the damping is 25 and the last output was 10
- **THEN** the trimmed mean is 20 and the node gives 17.5

#### Scenario: No damping
- **WHEN** the damping is 0 and the trimmed mean is 20
- **THEN** the node gives 20

### Requirement: Trend node
The trend node SHALL give the least-squares slope of the samples of the window against the sample number, rounded to 5 decimals. With fewer than 3 samples it SHALL give 0.

#### Scenario: Rising values
- **WHEN** the window holds 1, 2, 3, 4
- **THEN** the node gives 1

#### Scenario: Falling values
- **WHEN** the window holds 6, 4, 2
- **THEN** the node gives -2

#### Scenario: Two samples
- **WHEN** the window holds 5 and 9
- **THEN** the node gives 0

### Requirement: Signal generator nodes
The generators linear, sawtooth, triangle, sine and rectangle SHALL have the inputs enable, reset and period. While enable is true the phase t SHALL advance with the clock, and u SHALL be t / period modulo 1. Reset true SHALL set t to 0. The period SHALL round to 0.1 s and SHALL be at least 0.1 s. While enable is false the output SHALL be 0 and t SHALL hold.

#### Scenario: Shapes
- **WHEN** t = 4 s and the period is 10 s
- **THEN** the linear node gives 0.4, the sawtooth node 0.4, the triangle node 0.8, the sine node about 0.794 and the rectangle node 1

#### Scenario: Rectangle second half
- **WHEN** the period is 10 s and t = 7 s
- **THEN** the rectangle node gives 0

#### Scenario: Sine at the start
- **WHEN** the period is 10 s and t = 0
- **THEN** the sine node gives 0.5

#### Scenario: Linear stays at 1
- **WHEN** the period is 10 s and t = 25 s
- **THEN** the linear node gives 1

### Requirement: Comment node
The comment node SHALL hold a text of up to 1024 characters and a width and height on the canvas, with default 180 x 60. It SHALL have no pins. It SHALL not count as an end node.

#### Scenario: Comment in a Graph
- **WHEN** a Graph holds a comment node, a constant and a sensor node
- **THEN** the daemon accepts the Graph and evaluates the comment as nothing

### Requirement: Sensor end node
The sensor end node SHALL have one input pin, a unit and the flag "keep the value over a restart". A Graph with a sensor end node SHALL publish one sensor with the name of the Graph. The sensor SHALL have the value of the input pin, or no value when that pin has none.

#### Scenario: Value reaches the sensor
- **WHEN** the input pin of the sensor end node reads 12.5
- **THEN** the Graph sensor reads 12.5 in the same tick

#### Scenario: No value
- **WHEN** the input pin has no value
- **THEN** the Graph sensor has no value and a Curve that uses it follows the Lost Input rule

#### Scenario: Keep the value
- **WHEN** the flag is on and the daemon restarts
- **THEN** the sensor reads the stored value until the first evaluation gives a result

### Requirement: Graph sensor identity
A Graph with a sensor end node SHALL own one sensor id. A new Graph SHALL get the id `graph_<n>`. A converted Derived sensor SHALL keep its `derived_<n>` id. The daemon SHALL not reuse the id of a deleted Graph.

#### Scenario: New sensor Graph
- **WHEN** the user applies a new sensor Graph and the last number used was 4
- **THEN** the sensor id is `graph_5`

#### Scenario: Id after delete
- **WHEN** the user deletes `graph_5` and applies a new sensor Graph
- **THEN** the new sensor id is `graph_6`

#### Scenario: Deleted sensor used by a Curve
- **WHEN** the user deletes a Graph whose sensor is the Input of a Curve
- **THEN** the daemon deletes the Graph and the Curve has a lost Input

### Requirement: Graph sensor as a source
A Graph sensor SHALL be a normal sensor. It SHALL serve as the Input of a Curve, as the source of a Rule, as the source of a software sensor of a Leakshield, as a series of the app history and as the source of another Graph.

#### Scenario: Curve Input
- **WHEN** a Curve has a Graph sensor as Input and the Graph gives 55
- **THEN** the Curve computes its Duty for the Input 55 in the same tick

#### Scenario: Graph reads Graph
- **WHEN** Graph B has a sensor source node on the sensor of Graph A
- **THEN** Graph B reads the value that Graph A gave in the same tick

### Requirement: Evaluation rate and order
The daemon SHALL evaluate every enabled Graph once per control tick, after the Alarms and before the Rule groups. A Curve of the same tick SHALL see the new value. Inside a Graph, the sources and constants SHALL come first, then each node whose connected inputs are computed. A Graph that reads the sensor of another Graph SHALL run after it.

#### Scenario: Same tick
- **WHEN** a source sensor changes in tick N
- **THEN** the Graph sensor and the Curve that reads it use the new value in tick N

#### Scenario: Chain of Graphs
- **WHEN** Graph B reads the sensor of Graph A and A was written after B
- **THEN** the daemon still runs A before B

### Requirement: Disabled Graph
A disabled Graph SHALL give no value to its sensor and SHALL run no action. The daemon SHALL keep its nodes and stored values.

#### Scenario: Disable a sensor Graph
- **WHEN** the user disables a sensor Graph
- **THEN** its sensor has no value in the next tick

#### Scenario: Disable an action Graph
- **WHEN** the user disables an action Graph while its input is true
- **THEN** the action does not run

### Requirement: Clock
Windows, delays, generators and counters SHALL read the simulated clock of the core, so a test runs without waiting. A test clock that advances 1 s per tick SHALL give the same values as the control tick.

#### Scenario: Test without waiting
- **WHEN** a test advances the clock by 10 s in 10 steps after the input of a 3 s on delay node rose
- **THEN** the node has given 1 from the 4th step

### Requirement: Loop check inside a Graph
The daemon SHALL refuse a Graph with a connection that closes a loop, a connection between two input pins or two output pins, and a connection between two pins of one node. The refusal SHALL name the reason.

#### Scenario: Loop of two nodes
- **WHEN** a Graph write connects node X to node Y and node Y back to node X
- **THEN** the daemon refuses it with `InvalidArgument` and the reason `loop`

#### Scenario: Same direction
- **WHEN** a connection joins two output pins
- **THEN** the daemon refuses it with `InvalidArgument`

#### Scenario: Same node
- **WHEN** a connection joins the output pin and an input pin of one node
- **THEN** the daemon refuses it with `InvalidArgument`

### Requirement: Loop check across Graphs
The daemon SHALL refuse a Graph write that closes a loop through the sensors of several Graphs, a Graph that reads its own sensor included.

#### Scenario: Two Graphs read each other
- **WHEN** Graph A reads the sensor of Graph B and the user applies Graph B with a source on the sensor of Graph A
- **THEN** the daemon refuses the write with `InvalidArgument` and the reason `loop-across-graphs`

#### Scenario: Graph reads itself
- **WHEN** a sensor Graph has a source on its own sensor
- **THEN** the daemon refuses the write with `InvalidArgument`

### Requirement: Graph storage
The daemon SHALL store all Graphs in `graphs.json` in its state directory with the atomic write, the schema version and the last-known-good copy of the state store. A write SHALL return after the file is written. A failed file write SHALL fail the call and keep the Graph in memory as it was.

#### Scenario: Restart
- **WHEN** the user applies a Graph and the daemon restarts
- **THEN** the Graph has the same nodes, connections and sensor id

#### Scenario: Higher schema version
- **WHEN** `graphs.json` has a higher schema version than the daemon knows
- **THEN** the daemon loads the last-known-good copy, writes no Graph file and raises the `state-fallback` Warning

### Requirement: Stored values
The daemon SHALL store the values of the persistent storage nodes and of the sensor nodes with the keep flag in `graph-values.json`. It SHALL write the file at most every 30 s when a value changed, and at shutdown.

#### Scenario: Write rate
- **WHEN** a stored value changes every second for 60 s
- **THEN** the daemon writes `graph-values.json` at most 3 times in those 60 s

#### Scenario: Shutdown
- **WHEN** the daemon stops 5 s after the last write and a value changed
- **THEN** it writes the file before it exits

### Requirement: Apply resets state
A write of a Graph SHALL reset the state of its nodes except the stored values of nodes that keep their id and kind. The daemon SHALL check the Graph again with the rules of this spec.

#### Scenario: Window after Apply
- **WHEN** the user changes a constant in a Graph with a 60 s average filter and applies it
- **THEN** the filter starts with an empty window

#### Scenario: Stored value kept
- **WHEN** the user changes another node of a Graph and applies it
- **THEN** the persistent storage node keeps its stored value

### Requirement: Graph export
The app SHALL export one Graph to one JSON file. The file SHALL hold a format name and a format version, the name, the nodes with their parameters, labels and positions, and the connections. It SHALL hold no stored value and no password.

#### Scenario: Export file
- **WHEN** the user exports a Graph of 5 nodes
- **THEN** the file holds 5 nodes, the connections, the format name and the version

#### Scenario: No secret in the file
- **WHEN** the Graph has an e-mail action
- **THEN** the file holds the recipient, the subject and the text, and no SMTP password

### Requirement: Graph import
The daemon SHALL import a Graph from the content of one file of at most 1 MiB. It SHALL give the Graph and its nodes new ids, a new sensor id for a sensor Graph, and the state disabled. A sensor source that does not exist on this machine SHALL stay in the Graph as an unbound source. A file with a higher format version SHALL fail with `InvalidArgument`.

#### Scenario: Import twice
- **WHEN** the user imports the same file twice
- **THEN** two Graphs exist with different ids and different sensor ids

#### Scenario: Unknown sensor
- **WHEN** the file has a source on `nct9999_temp7` and that sensor does not exist
- **THEN** the Graph imports and the node is unbound

#### Scenario: Imported action stays off
- **WHEN** the user imports a Graph with a command action
- **THEN** the Graph is disabled until the user enables it

### Requirement: Conversion of Derived sensors
At the first start with this change the daemon SHALL convert each stored Derived sensor to a Graph with the same sensor id and the same name. A maximum of N sources SHALL become N - 1 maximum nodes in a chain. A mean of N sources SHALL become N - 1 addition nodes in a chain and one division by the constant N. A difference SHALL become one subtraction node. The unit SHALL stay.

#### Scenario: Maximum of three
- **WHEN** `derived_1` is the maximum of three sensors
- **THEN** the Graph has 3 sensor source nodes, 2 maximum nodes and a sensor end node, and the sensor id is `derived_1`

#### Scenario: Mean of two
- **WHEN** `derived_2` is the mean of two sensors that read 40 and 50
- **THEN** the Graph gives 45

#### Scenario: Difference
- **WHEN** `derived_3` is the difference of 60 minus 45
- **THEN** the Graph gives 15

#### Scenario: Curve keeps its Input
- **WHEN** a Curve uses `derived_1` as Input before the update
- **THEN** the Curve uses `derived_1` after the update with no change

### Requirement: Conversion safety
The daemon SHALL copy the state file with the Derived sensors to a backup file before it removes them from the state. The conversion SHALL run once. A source with no value SHALL give no value, as the Derived sensor did.

#### Scenario: Backup exists
- **WHEN** the conversion has run
- **THEN** the backup file holds the old Derived sensor list and the state no longer holds it

#### Scenario: Second start
- **WHEN** the daemon starts again
- **THEN** it converts nothing and creates no second Graph for a Derived sensor

#### Scenario: Missing source
- **WHEN** a source of a converted maximum has no value
- **THEN** the Graph sensor has no value

### Requirement: Removal of the Derived sensor API
The daemon SHALL not expose the Derived sensor methods after this change. The Sensors screen SHALL open the Graph editor for a Graph sensor.

#### Scenario: Old method
- **WHEN** a client calls `CreateDerivedSensor`
- **THEN** the daemon answers with an unknown method error

#### Scenario: Sensors screen
- **WHEN** the user opens a Graph sensor from the Sensors screen
- **THEN** the app opens the Graph editor on its Graph

### Requirement: Graph D-Bus objects
The daemon SHALL expose each Graph as an object `graph/<id>` with the properties name, enabled, end kind, sensor id and state. The root object SHALL give operations to apply (create or replace), enable, delete, import, export, read node values, test a draft, test an action and read the action log of a Graph. The state SHALL be `running`, `disabled` or `unbound`.

#### Scenario: Object appears
- **WHEN** the user applies a new Graph
- **THEN** the daemon emits `InterfacesAdded` for its object

#### Scenario: Node values
- **WHEN** a client reads the node values of a running Graph
- **THEN** the reply holds one entry per node with its value or a no-value flag

#### Scenario: Unbound state
- **WHEN** a Graph has an unbound source node
- **THEN** its state is `unbound`

### Requirement: Graph authorization
Graph read methods SHALL need no polkit action. Graph write methods SHALL need the polkit action `io.github.cappyt.NiceCooling1.write`. No method SHALL ask for a password beyond the polkit rules of that action. The write methods SHALL be apply, enable, delete, import, test an action, and the settings of the broker and the SMTP account.

#### Scenario: Read by any user
- **WHEN** a user outside `wheel` reads the node values
- **THEN** the call succeeds with no dialog

#### Scenario: Apply by a wheel member
- **WHEN** a `wheel` member in the active session applies a Graph
- **THEN** the call succeeds under the rule of `write`

#### Scenario: Apply by another user
- **WHEN** a user outside `wheel` applies a Graph
- **THEN** polkit shows its dialog and the call fails when the user cancels

### Requirement: Test of a draft
The daemon SHALL evaluate a draft Graph on a copy for a set time of 1 to 600 s, default 60 s, with given test values for sensor source nodes. At most 4 tests SHALL run at once. A test SHALL return the node values each second. A test SHALL not change a published sensor value and SHALL run no action.

#### Scenario: Test value
- **WHEN** a draft has a sensor source node with the test value 80 and an addition node with a constant 5
- **THEN** the test reads 85 on the addition node and the real sensor is unchanged

#### Scenario: Test ends
- **WHEN** the set time passes
- **THEN** the daemon deletes the copy and the next read fails with `NotFound`

#### Scenario: Fifth test
- **WHEN** 4 tests run and a client starts a fifth
- **THEN** the daemon refuses it with `InvalidArgument`

#### Scenario: Curve not touched
- **WHEN** a test runs on a Graph whose sensor is the Input of a Curve
- **THEN** the Curve still reads the published value

### Requirement: Graphs list screen
The app SHALL show a screen "Graphs" in the group "Control". It SHALL list each Graph with its name, its end node kind, its state and its live result. It SHALL offer "New sensor", "New action" and "Import". Each row SHALL offer an enable switch, "Edit", "Export" and "Delete" with a confirmation.

#### Scenario: Live result of a sensor Graph
- **WHEN** a sensor Graph gives 46.2 in degrees Celsius
- **THEN** its row shows "46.2 °C" and its state "Running"

#### Scenario: Live result of an action Graph
- **WHEN** an action node is in the state "run"
- **THEN** its row shows "Fired" and the time of its last run

#### Scenario: Delete a used sensor
- **WHEN** the user deletes a Graph whose sensor is used by a Curve
- **THEN** the confirmation names the Curve and the delete goes through after the user confirms

### Requirement: Editor canvas
The editor SHALL show the nodes, the pins and the connections on a canvas. The user SHALL pan with a drag on the empty canvas, zoom from 25% to 400% with the wheel, move a node by drag, and connect an output pin to an input pin by drag. The editor SHALL show the reason when a connection is refused. Delete SHALL remove the selected node or connection, except the end node.

#### Scenario: Connect
- **WHEN** the user drags from an output pin to a free input pin
- **THEN** the editor draws the connection

#### Scenario: Refused connection
- **WHEN** the user drops a connection on an input pin that already has one
- **THEN** the editor draws nothing and shows the reason "input already connected"

#### Scenario: Zoom limit
- **WHEN** the user zooms out past 25%
- **THEN** the zoom stays at 25%

### Requirement: Editor node list
The editor SHALL show a node list grouped by family: sources, math, logic, state and time, filters, generators and other. It SHALL have a search field. A new node SHALL appear at the centre of the view by double click or by drag onto the canvas. The list SHALL disable the end node kinds because the Graph has its end node.

#### Scenario: Search
- **WHEN** the user types "delay"
- **THEN** the list shows the on delay and off delay nodes

#### Scenario: Add by double click
- **WHEN** the user double-clicks "Constant"
- **THEN** the canvas gets a constant node at the centre of the view

### Requirement: Editor live values
The editor SHALL show the value of each node on the node, with `--` for no value. It SHALL read the node values from the daemon once per second while it is open and SHALL stop reading when it closes.

#### Scenario: Live value
- **WHEN** a node gives 46.2
- **THEN** the node shows "46.2" within 2 seconds

#### Scenario: No value
- **WHEN** a node has no value
- **THEN** the node shows "--"

#### Scenario: Editor closed
- **WHEN** the user closes the editor
- **THEN** the app makes no more node value reads

### Requirement: Editor test value
A sensor source node SHALL offer a test value and a test time. The editor SHALL start a test of the draft with these values, show the node values of the test, show the banner "Test running, no value is published", and stop on the end of the time or on "Stop".

#### Scenario: Run a test
- **WHEN** the user gives the source 80 for 30 s and starts the test
- **THEN** the nodes show the values of the test and the banner is visible

#### Scenario: Stop
- **WHEN** the user presses "Stop"
- **THEN** the banner goes away and the nodes show the live values again

### Requirement: Editor undo and redo
The editor SHALL keep 100 steps of undo and redo for the draft. One drag SHALL be one step. One committed parameter edit SHALL be one step. Ctrl+Z SHALL undo and Ctrl+Shift+Z SHALL redo.

#### Scenario: Undo a node move
- **WHEN** the user moves a node and presses Ctrl+Z
- **THEN** the node returns to its earlier position

#### Scenario: Redo
- **WHEN** the user undoes a delete and then presses Ctrl+Shift+Z
- **THEN** the node is deleted again

### Requirement: Editor draft and Apply
The editor SHALL work on a draft. The daemon SHALL receive the Graph only at "Apply". The button SHALL be off when nothing changed, when the draft breaks a model rule, or when the daemon is offline. The editor SHALL show the first broken rule. Leaving with an unapplied draft SHALL ask: Apply, Discard or Cancel.

#### Scenario: Apply
- **WHEN** the user presses "Apply" on a valid draft
- **THEN** the daemon holds the Graph and the button turns off

#### Scenario: Broken rule
- **WHEN** the draft has two end nodes
- **THEN** "Apply" is off and the editor shows "A Graph has one end node"

#### Scenario: Leave with a draft
- **WHEN** the user closes the editor with an unapplied change
- **THEN** the app asks Apply, Discard or Cancel

#### Scenario: Daemon refuses
- **WHEN** the daemon refuses the Graph at Apply
- **THEN** the editor keeps the draft and shows the reason

### Requirement: Editor states
The editor SHALL show these states. Offline: a banner, dim values, the draft stays and "Apply" is off. Read error: a message with "Retry"; a failed Graph read opens no editor, and a failed value read shows dim `--`. Unbound source: the node shows a warning icon, the stored sensor id and "Sensor not found", and the user can choose another sensor.

#### Scenario: Daemon goes offline
- **WHEN** the daemon becomes unreachable while the editor is open
- **THEN** the editor shows the banner, keeps the draft and turns "Apply" off

#### Scenario: Graph read fails
- **WHEN** the read of a Graph fails with an error
- **THEN** the app shows the error and "Retry" and opens no editor

#### Scenario: Unbound source
- **WHEN** a sensor source node holds an id that does not exist
- **THEN** the node shows the warning icon and the id, and "Apply" stays on

### Requirement: Editor style and keyboard
The editor SHALL use the theme tokens of the app in the light and dark theme. Every control SHALL work with the keyboard rules of the app: Tab order, Enter to confirm, Escape to cancel an edit.

#### Scenario: Dark theme
- **WHEN** the system theme is dark
- **THEN** the canvas, nodes and pins use the dark tokens

#### Scenario: Escape
- **WHEN** the user edits a parameter and presses Escape
- **THEN** the parameter keeps its earlier value
