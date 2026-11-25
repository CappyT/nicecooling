# Node graph

## Purpose

Defines the Graph: a set of nodes and connections that the app evaluates once per second in the user session and that ends in one sensor end node or one response end node. It covers the no-value rule, the arithmetic of every node kind, the loop checks, the Graph sensor and its values in the daemon, storage, export and import, and the Graph editor.

## ADDED Requirements

### Requirement: Graph model
A Graph SHALL have an id, a name, an enabled flag, nodes and connections. A Graph with a sensor end node SHALL have a `sensor_id` of the form `graph_<n>`, or none. A node SHALL have an id (a positive integer, unique in the Graph), a kind, an optional label, parameters and a canvas position. A connection SHALL join one output pin to one input pin. An input pin SHALL take one connection at most. An output pin SHALL take any number.

#### Scenario: Two connections on one input pin
- **WHEN** a Graph holds two connections that end on the same input pin
- **THEN** the validator refuses the Graph with the code `pin-occupied`

#### Scenario: One output, two nodes
- **WHEN** one output pin has connections to two input pins of two nodes
- **THEN** the validator accepts the Graph and both nodes read the same value

#### Scenario: Sensor link
- **WHEN** a Graph with a sensor end node has the `sensor_id` `graph_3`
- **THEN** its sensor end node gives the value of the Graph sensor `graph_3`

### Requirement: Validator refusal codes
The validator SHALL name each refusal with one code: `no-end-node`, `two-end-nodes`, `end-node-output`, `unknown-kind`, `unknown-pin`, `pin-occupied`, `same-direction`, `same-node`, `loop`, `loop-across-graphs`, `limit-graphs`, `limit-nodes`, `limit-name`, `limit-samples`, `limit-graph-sensors`, `duplicate-label`, `duplicate-table-input`, `bad-parameter` (with the name of the parameter) and `bad-sensor-id`. It SHALL return every refusal of a Graph, the first in the order of this list first.

#### Scenario: One refusal
- **WHEN** a Graph has no end node
- **THEN** the validator returns `no-end-node`

#### Scenario: Two refusals
- **WHEN** a Graph has two end nodes and a random node with min 5 and max 1
- **THEN** the validator returns `two-end-nodes` and `bad-parameter` for `max`, in this order

#### Scenario: Duplicate label
- **WHEN** two nodes of a Graph have the label `water`
- **THEN** the validator returns `duplicate-label`

#### Scenario: Sensor id
- **WHEN** a Graph with a response end node has a `sensor_id`
- **THEN** the validator returns `bad-sensor-id`

### Requirement: One end node
A Graph SHALL have exactly one end node: a sensor end node or a response end node. An end node SHALL have no output pin. The validator SHALL refuse a Graph with no end node or with two, and a connection that starts on an end node.

#### Scenario: Graph without an end node
- **WHEN** a Graph holds a constant node and an addition node and no end node
- **THEN** the validator refuses the Graph with the code `no-end-node`

#### Scenario: Graph with two end nodes
- **WHEN** a Graph holds a sensor end node and a response end node
- **THEN** the validator refuses the Graph with the code `two-end-nodes`

#### Scenario: Connection from an end node
- **WHEN** a Graph holds a connection that starts on an end node
- **THEN** the validator refuses the Graph with the code `end-node-output`

### Requirement: Graph limits
The app SHALL store at most 64 Graphs and at most 256 nodes in one Graph. A Graph name SHALL have at most 64 characters. A node label SHALL have 1 to 32 characters of letters, digits and `_`, and SHALL be unique in its Graph. The validator SHALL refuse a Graph above a limit. The limits of the window samples and of the Graph sensors are in their own requirements.

#### Scenario: 65th Graph
- **WHEN** 64 Graphs exist and the user applies a new Graph
- **THEN** the validator refuses it with the code `limit-graphs`

#### Scenario: 257 nodes
- **WHEN** a Graph holds 257 nodes
- **THEN** the validator refuses it with the code `limit-nodes`

#### Scenario: Name of 65 characters
- **WHEN** a Graph has a name of 65 characters
- **THEN** the validator refuses it with the code `limit-name`

#### Scenario: Label with a space
- **WHEN** a node has the label `hot water`
- **THEN** the validator refuses it with the code `bad-parameter` for `label`

### Requirement: Parameter types
Every time parameter SHALL be a whole number of seconds, except the period of a generator and of the counter, which is a number of at least 0.1. A comment node SHALL have a width of 60 to 800 and a height of 20 to 600. An e-mail address SHALL have one `@` and at most 254 characters. A command SHALL have at most 32 arguments of at most 1024 characters each. A sound name SHALL have at most 64 characters. A required parameter that a stored node leaves out SHALL be refused like an empty one.

#### Scenario: Missing required parameter
- **WHEN** a stored response node of kind `sound` has no `sound` parameter
- **THEN** the validator refuses it with the code `bad-parameter` for `sound`

#### Scenario: Fraction of a second
- **WHEN** a response has a hold time of 2.5 s
- **THEN** the validator refuses it with the code `bad-parameter` for `hold_s`

#### Scenario: Counter period
- **WHEN** the counter has the period 0.5 s
- **THEN** the validator accepts it

#### Scenario: Comment too wide
- **WHEN** a comment node has the width 900
- **THEN** the validator refuses it with the code `bad-parameter` for `width`

#### Scenario: Address without an @
- **WHEN** an e-mail response has the address `owner.example.org`
- **THEN** the validator refuses it with the code `bad-parameter` for `to`

#### Scenario: 33 arguments
- **WHEN** a command response has 33 arguments
- **THEN** the validator refuses it with the code `bad-parameter` for `args`

### Requirement: Window sample limit
The Graphs together SHALL hold at most 200 000 window samples. A window node SHALL hold one sample per whole second of the monotonic clock: a second sample in the same second replaces the first. So it holds at most its span in seconds plus 1 samples. The validator SHALL add this number for each window node of every Graph, and SHALL refuse a total above the limit. A sample takes at most 24 bytes, so the limit bounds the memory to 4.8 MB.

#### Scenario: 111 nodes of 1800 s
- **WHEN** the Graphs hold 111 window nodes with the span 1800 s
- **THEN** the total is 199 911 samples and the validator accepts it

#### Scenario: 112 nodes of 1800 s
- **WHEN** the Graphs hold 112 window nodes with the span 1800 s
- **THEN** the total is 201 712 samples and the validator refuses the Graph that adds the 112th with the code `limit-samples`

#### Scenario: Two evaluations in one second
- **WHEN** a window node with the span 3 s is evaluated twice in each second for 20 s
- **THEN** it holds at most 4 samples

#### Scenario: Default spans
- **WHEN** a Graph holds 100 window nodes with the span 10 s
- **THEN** the total is 1 100 samples

### Requirement: No-value rule
A pin SHALL carry a number or no value. A node with a connected input pin that has no value SHALL give no value and SHALL keep its state. This SHALL hold for every kind except the node "has value". A division by 0, a function outside its domain and a result that is not finite SHALL give no value.

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
An input pin of a node that is not an end node, with no connection, SHALL take the default of its kind: 0, except the base of the logarithm (the number e), the exponent of the power (2), and the enable pin (1) and the period pin (10 s) of the generators and the counter. The input pin of an end node SHALL have no default.

#### Scenario: Unconnected base
- **WHEN** a logarithm node has A connected to the constant 1 and B unconnected
- **THEN** the node gives 0, the natural logarithm of 1

#### Scenario: Unconnected exponent
- **WHEN** a power node has A connected to the constant 3 and B unconnected
- **THEN** the node gives 9

#### Scenario: Unconnected addition input
- **WHEN** an addition node has A connected to the constant 5 and B unconnected
- **THEN** the node gives 5

#### Scenario: Sensor end node with no connection
- **WHEN** a sensor end node has no connection
- **THEN** its Graph sensor has no value

#### Scenario: Response end node with no connection
- **WHEN** a response end node has no connection
- **THEN** its input counts as false

### Requirement: Sensor source node
The sensor source node SHALL give the current value of the chosen sensor, rounded to 8 decimals. The app SHALL read the value from the sensor events of the daemon. The node SHALL give no value while the daemon reports the sensor with no value. It SHALL accept any sensor, a Graph sensor and a Derived sensor included.

#### Scenario: Live sensor
- **WHEN** the chosen sensor reads 46.123456789
- **THEN** the node gives 46.12345679

#### Scenario: Rounding down
- **WHEN** the chosen sensor reads 46.1234567826
- **THEN** the node gives 46.12345678

#### Scenario: Exact value
- **WHEN** the chosen sensor reads 46.5
- **THEN** the node gives 46.5

#### Scenario: Stale sensor
- **WHEN** the daemon reports the chosen sensor with the no-value flag
- **THEN** the node gives no value

#### Scenario: Daemon offline
- **WHEN** the app has no connection to the daemon
- **THEN** every sensor source node gives no value

#### Scenario: Sensor that does not exist
- **WHEN** the chosen sensor id is not a sensor of this machine
- **THEN** the node is unbound and gives no value

### Requirement: Alarm event state source node
The alarm event state source node SHALL have an event id and a source id. It SHALL give 1 while the daemon reports an Alarm or Warning instance of that event for that source as active, and 0 otherwise. It SHALL give no value while the daemon is offline.

#### Scenario: Pump-stop Alarm active
- **WHEN** the node is set to `pump-stop` of `loop_1` and that Alarm is active
- **THEN** the node gives 1

#### Scenario: Other Loop
- **WHEN** the node is set to `pump-stop` of `loop_1` and the Alarm is active only for `loop_2`
- **THEN** the node gives 0

#### Scenario: Daemon offline
- **WHEN** the app loses the daemon
- **THEN** the node gives no value

### Requirement: Constant and random nodes
The constant node SHALL give its value, a finite number with default 0. The random node SHALL give a new number in its range each tick, rounded to 3 decimals. The range SHALL have a minimum and a maximum with minimum not above maximum. The default range SHALL be -100 to 100.

#### Scenario: Constant
- **WHEN** a constant node has the value 2.5
- **THEN** it gives 2.5 in every tick

#### Scenario: Random range
- **WHEN** a random node has the range 10 to 20 and runs for 1000 ticks
- **THEN** every value is between 10 and 20 and has at most 3 decimals

#### Scenario: Inverted range
- **WHEN** a Graph has a random node with minimum 5 and maximum 1
- **THEN** the validator refuses the Graph

### Requirement: Process node
The process node SHALL give 1 while a process with the chosen name runs, and 0 otherwise. It SHALL use the `/proc` scan of the Profile rules and the same name match, run as the user of the app. The app reads the `comm` of every process and the program path of the processes of the user only. The name SHALL have 1 to 255 characters.

#### Scenario: Process runs
- **WHEN** the node has the name `blender` and a process with the `comm` value `Blender` runs
- **THEN** the node gives 1

#### Scenario: Process of another user by path
- **WHEN** the name is `render-tool` and only the program path of a process of another user has that name
- **THEN** the node gives 0

#### Scenario: Process ends
- **WHEN** the last process of that name ends
- **THEN** the node gives 0 in the next evaluation

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
The round node SHALL round A to 0 to 8 decimals in one of three modes: to the nearest, down or up. The nearest mode SHALL round a tie away from 0. The defaults SHALL be 0 decimals and the nearest mode. All rounding of the nodes SHALL work on the shortest decimal text that reads back to the same number, so 0.29 rounded down to 2 decimals gives 0.29.

#### Scenario: Nearest
- **WHEN** a round node has 1 decimal, the nearest mode and A = 2.25
- **THEN** the node gives 2.3

#### Scenario: Down
- **WHEN** a round node has 0 decimals, the down mode and A = -1.5
- **THEN** the node gives -2

#### Scenario: Up
- **WHEN** a round node has 2 decimals, the up mode and A = 3.141
- **THEN** the node gives 3.15

#### Scenario: Down on a decimal step
- **WHEN** a round node has 2 decimals, the down mode and A = 0.29
- **THEN** the node gives 0.29

#### Scenario: Down at 8 decimals
- **WHEN** a round node has 8 decimals, the down mode and A = 1.123456789
- **THEN** the node gives 1.12345678

#### Scenario: Large value
- **WHEN** a round node has 0 decimals, the nearest mode and A = 3000000000.3
- **THEN** the node gives 3000000000

### Requirement: Limiter node
The limiter node SHALL hold A between a minimum and a maximum. The defaults SHALL be -100 and 100. The validator SHALL refuse a minimum above the maximum.

#### Scenario: Above the maximum
- **WHEN** a limiter node has the range 0 to 80 and A = 95
- **THEN** the node gives 80

#### Scenario: Inside the range
- **WHEN** a limiter node has the range 0 to 80 and A = 42
- **THEN** the node gives 42

### Requirement: Table node
The table node SHALL map A through 2 to 64 pairs of an input and an output with the interpolation function that it shares with the Curve. The validator SHALL sort the pairs by input and refuse two pairs with the same input. The mode SHALL be linear or step. A at or below the first input SHALL give the first output. A at or above the last input SHALL give the last output. The default pairs SHALL be (-100, -100) and (100, 100).

#### Scenario: Linear between two pairs
- **WHEN** the pairs are (20, 30) and (40, 80), the mode is linear and A = 30
- **THEN** the node gives 55

#### Scenario: Step mode
- **WHEN** the pairs are (20, 30) and (40, 80), the mode is step and A = 39.9
- **THEN** the node gives 30

#### Scenario: Above the last input
- **WHEN** the pairs are (20, 30) and (40, 80) and A = 90
- **THEN** the node gives 80

#### Scenario: Input that is not a number
- **WHEN** A is not a number
- **THEN** the node gives no value

#### Scenario: Same input twice
- **WHEN** a Graph has two pairs with the input 20
- **THEN** the validator refuses it with the code `duplicate-table-input`

### Requirement: Comparison nodes
The nodes less, less or equal, equal, greater or equal, greater and not equal SHALL compare A with B as the `Comparison` of the Profile rules does and SHALL give 1 when true and 0 when false. Equal SHALL hold when the difference is at most 1e-9. Not equal SHALL hold when the difference is above 1e-9. The other four SHALL compare exactly.

#### Scenario: Greater
- **WHEN** a greater node has A = 40.1 and B = 40
- **THEN** the node gives 1

#### Scenario: Equal with tolerance
- **WHEN** an equal node has A = 0.1 + 0.2 as computed by a node and B = 0.3
- **THEN** the node gives 1

#### Scenario: Equal outside the tolerance
- **WHEN** an equal node has A = 5 and B = 5.000001
- **THEN** the node gives 0

#### Scenario: Not equal
- **WHEN** a not equal node has A = 5 and B = 5
- **THEN** the node gives 0

#### Scenario: Same result as a Profile rule
- **WHEN** a Profile rule and a Graph compare the same sensor with the value 50 and the sensor reads 50.0000000001
- **THEN** both say equal

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

### Requirement: Has value node
The node "has value" SHALL give 1 when its input has a value and 0 when its input has no value. It SHALL be the one exception to the no-value rule. With no connection it SHALL give 0.

#### Scenario: Value present
- **WHEN** the input gives 12
- **THEN** the node gives 1

#### Scenario: Value lost
- **WHEN** the input is a sensor source node whose sensor has no value
- **THEN** the node gives 0 and the node after it gives a value

#### Scenario: Not connected
- **WHEN** the input pin has no connection
- **THEN** the node gives 0

### Requirement: Memory node
The memory node SHALL take A while S is true and SHALL hold the last taken value while S is false. Its start value SHALL be 0. It SHALL start again at 0 after a restart of the app.

#### Scenario: Take and hold
- **WHEN** S is 1 with A = 5, then S is 0 with A = 9
- **THEN** the node gives 5 in both evaluations

#### Scenario: Restart
- **WHEN** the app restarts after the node held 5
- **THEN** the node gives 0 until S is true

### Requirement: Persistent storage node
The persistent storage node SHALL work as the memory node. It SHALL also keep its value over a restart of the app in `graph-state.json`. It SHALL give no value while a connected input has none.

#### Scenario: Restart
- **WHEN** the app restarts after the node held 5 and S stays 0
- **THEN** the node gives 5

#### Scenario: Input without value
- **WHEN** S has no value
- **THEN** the node gives no value and keeps 5

#### Scenario: Node removed
- **WHEN** the user applies the Graph without the node
- **THEN** the app deletes the stored value of the node

### Requirement: Trigger node
The trigger node SHALL give 1 for one tick when A goes from false to true, and 0 otherwise. The first tick with a value SHALL give 0 and SHALL record A.

#### Scenario: Rising edge
- **WHEN** A is 0 for 2 ticks, then 1 for 3 ticks
- **THEN** the node gives 0, 0, 1, 0, 0

#### Scenario: True at the start
- **WHEN** A is 1 in the first tick with a value
- **THEN** the node gives 0 in that tick

### Requirement: On delay and off delay nodes
The on delay node SHALL give 1 when A has been true for at least the delay, and 0 otherwise. The off delay node SHALL give 1 while A is true and for the delay after A becomes false. The delay SHALL be 0 to 3600 s, default 1 s. Both nodes SHALL start at 0. A false input and an input with no value SHALL reset the time.

#### Scenario: On delay
- **WHEN** the delay is 3 s and A becomes 1 at second 10 and stays 1
- **THEN** the node gives 0 at seconds 10, 11 and 12 and gives 1 from second 13

#### Scenario: On delay reset by no value
- **WHEN** A is 1 from second 10, has no value at second 12 and is 1 again from second 13
- **THEN** the node gives no value at second 12 and gives 0 until second 15, then 1 from second 16

#### Scenario: Off delay
- **WHEN** the delay is 3 s, A is 1 until second 10 and 0 from second 11
- **THEN** the node gives 1 until second 13 and 0 from second 14

#### Scenario: Off delay reset by no value
- **WHEN** A is 1 until second 10, has no value from second 11
- **THEN** the node gives no value from second 11 and drops the delay

### Requirement: Counter node
The counter node SHALL have the inputs count, enable, reset and period. While enable is true it SHALL add count to its output once each period. Reset true SHALL set the output and the timer to 0. The output SHALL start at 0. It SHALL add at most once per tick.

#### Scenario: Count every 5 seconds
- **WHEN** count is 2, period is 5 s and enable stays 1 for 12 s
- **THEN** the output is 0 at second 0, 2 at second 5 and 4 at second 10

#### Scenario: Reset
- **WHEN** reset is 1 in a tick
- **THEN** the output is 0 in that tick

#### Scenario: Disabled
- **WHEN** enable is 0
- **THEN** the output holds and the timer does not advance

### Requirement: Window filter nodes
The peak, minimum and average filter nodes SHALL give the largest, the smallest and the mean of the samples of a time window, rounded to 5 decimals. The span SHALL be 1 to 1800 s, default 10 s. Each second the node SHALL remove every sample older than the span. While its input has no value the node SHALL give no value and add no sample, and its samples SHALL still age.

#### Scenario: Old samples leave
- **WHEN** the span is 3 s and the values 1, 2, 3, 4, 5 arrive at seconds 0 to 4
- **THEN** the average node gives 3.5 at second 4 because the sample 1 left the window

#### Scenario: Gap
- **WHEN** the span is 3 s, the values 1, 2, 3 arrive at seconds 0 to 2, there is no value until second 9, and 7 arrives at second 10
- **THEN** the average node gives 7 at second 10

#### Scenario: Peak
- **WHEN** the span is 10 s and the values 4, 9, 6 arrive
- **THEN** the peak node gives 9 after the third value

#### Scenario: No value
- **WHEN** A has no value at second 5
- **THEN** the node gives no value and adds no sample

### Requirement: Low pass node
The low pass node SHALL use the window of the filter nodes. With fewer than 3 samples it SHALL give their mean. Otherwise it SHALL sort the samples, drop the lowest and the highest in pairs while at least a third of the samples would remain, take the mean of the rest, and move its output toward that mean by the factor 1 - damping / 100. Damping SHALL be 0 to 99.99, default 25. After a gap that empties the window, the first output SHALL be the first mean again.

#### Scenario: First samples
- **WHEN** the window holds the samples 10 and 20
- **THEN** the node gives 15

#### Scenario: Damped step
- **WHEN** the window holds 10, 20 and 30, the damping is 25 and the last output was 10
- **THEN** the trimmed mean is 20 and the node gives 17.5

#### Scenario: After a gap
- **WHEN** the window emptied because A had no value for longer than the span, and then the samples 10, 10 and 10 arrive
- **THEN** the node gives 10 and not a value that moves from the output before the gap

#### Scenario: No damping
- **WHEN** the damping is 0 and the trimmed mean is 20
- **THEN** the node gives 20

#### Scenario: No value
- **WHEN** A has no value
- **THEN** the node gives no value

### Requirement: Trend node
The trend node SHALL give the least-squares slope of the samples of the window against the sample number, rounded to 5 decimals. With fewer than 3 samples it SHALL give 0. While its input has no value it SHALL give no value.

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
The generators linear, sawtooth, triangle, sine and rectangle SHALL have the inputs enable, reset and period. While enable is true the phase t SHALL advance with the clock. Reset true SHALL set t to 0. The period SHALL round to 0.1 s and SHALL be at least 0.1 s. While enable is false the output SHALL be 0 and t SHALL hold.

#### Scenario: Disabled generator
- **WHEN** enable is 0 after t reached 4 s
- **THEN** the output is 0 and t stays 4 s

#### Scenario: Period rounding
- **WHEN** the period input is 0.04 s
- **THEN** the node uses 0.1 s

#### Scenario: Reset
- **WHEN** reset is 1 at t = 4 s
- **THEN** t is 0

### Requirement: Generator shapes
With u = t / period modulo 1, the outputs SHALL be: linear min(t / period, 1), which rises once from 0 to 1 and stays at 1; sawtooth u; triangle 2u for u below 0.5 and 2(1 - u) otherwise; sine (1 + sin(2 pi t / period)) / 2; rectangle 1 for u below 0.5 and 0 otherwise. The sawtooth, triangle, sine and rectangle SHALL repeat each period.

#### Scenario: Shapes at 4 s
- **WHEN** t = 4 s and the period is 10 s
- **THEN** the linear node gives 0.4, the sawtooth 0.4, the triangle 0.8, the sine about 0.794 and the rectangle 1

#### Scenario: Second period
- **WHEN** t = 14 s and the period is 10 s
- **THEN** the sawtooth node gives 0.4 and the linear node gives 1

#### Scenario: Rectangle second half
- **WHEN** the period is 10 s and t = 7 s
- **THEN** the rectangle node gives 0

#### Scenario: Sine at the start
- **WHEN** the period is 10 s and t = 0
- **THEN** the sine node gives 0.5

### Requirement: Comment node
The comment node SHALL hold a text of up to 1024 characters and a width and height on the canvas, with default 180 x 60. It SHALL have no pins. It SHALL not count as an end node.

#### Scenario: Comment in a Graph
- **WHEN** a Graph holds a comment node, a constant and a sensor node
- **THEN** the validator accepts the Graph and evaluates the comment as nothing

### Requirement: Sensor end node
The sensor end node SHALL have one input pin and a unit. Its value SHALL be the value of the Graph sensor of the Graph. It SHALL give no value when the input pin has none. The unit SHALL be one of the units of the requirement "Graph sensor units". The user SHALL be able to change the unit and the name while the Graph sensor exists.

#### Scenario: Value reaches the sensor
- **WHEN** the input pin reads 12.5
- **THEN** the app sends 12.5 for the Graph sensor in the same evaluation

#### Scenario: No value
- **WHEN** the input pin has no value
- **THEN** the app sends the no-value entry and a Curve that uses the sensor follows the Lost Input rule

#### Scenario: Unit change
- **WHEN** the user changes the unit of an applied Graph with a sensor end node from `degC` to `%`
- **THEN** the app calls `GraphSensor.SetDefinition` with the new unit

### Requirement: Response end node
The response end node SHALL have one input pin and the settings of the graph-responses spec. Its input SHALL be true above 0. A missing connection SHALL count as false. An input with no value SHALL leave the state of the node as it is and SHALL start its hold time again.

#### Scenario: Input above zero
- **WHEN** the input pin reads 1
- **THEN** the response end node sees true

#### Scenario: Input without value
- **WHEN** the node is in the state "run" and the input has no value for 5 s and then reads 1
- **THEN** the node stays in the state "run" and does not run again

#### Scenario: No connection
- **WHEN** the input pin has no connection
- **THEN** the node sees false

### Requirement: Evaluation in the app
The app SHALL evaluate every enabled Graph once per second in a service thread of its own, with the sensor values of the newest sensor events of the daemon. The daemon SHALL run no Graph code. Inside a Graph, the sources and constants SHALL come first, then each node whose connected inputs are computed. A Graph that reads the Graph sensor of another Graph SHALL run after it and SHALL read its value of the same pass.

#### Scenario: Chain of Graphs
- **WHEN** Graph B reads the Graph sensor of Graph A and A was applied after B
- **THEN** the app still evaluates A before B and B reads the value of A from the same second

#### Scenario: App stopped
- **WHEN** the app does not run
- **THEN** no Graph is evaluated and the daemon runs its control tick as before

### Requirement: Disabled Graph
A disabled Graph SHALL not be evaluated. Its Graph sensor SHALL get the no-value entry and its response SHALL not run. The app SHALL keep its nodes and stored values.

#### Scenario: Disable a Graph with a sensor end node
- **WHEN** the user disables a Graph with a sensor end node
- **THEN** the app sends the no-value entry for its sensor from the next second

#### Scenario: Disable a Graph with a response end node
- **WHEN** the user disables a Graph with a response end node while its input is true
- **THEN** the response does not run

### Requirement: Clock
Delays, windows, generators, counters, hold times and repeat intervals SHALL count seconds of the monotonic clock of the app, which does not count a suspend. The minimum interval of a response SHALL use the wall clock. The Response pause SHALL count run time of the app. A test SHALL replace both clocks.

#### Scenario: Suspend
- **WHEN** a 60 s window holds 30 s of samples and the PC suspends for 1 hour
- **THEN** after the resume the window still holds 30 s of samples

#### Scenario: Test without waiting
- **WHEN** a test advances the clock by 10 s in 10 steps after the input of a 3 s on delay node rose
- **THEN** the node has given 1 from the 4th step

### Requirement: Fault isolation
The app SHALL evaluate each Graph inside a panic boundary. A Graph whose evaluation panics SHALL be disabled and stored as disabled. The app SHALL show a notification that names the Graph. Its Graph sensor SHALL have no value.

#### Scenario: Panic in one Graph
- **WHEN** Graph B panics in an evaluation and Graph A does not
- **THEN** the app disables B, shows "Graph B stopped" and keeps evaluating A

#### Scenario: Restart after a panic
- **WHEN** the app restarts after the panic
- **THEN** Graph B is still disabled

### Requirement: Loop check inside a Graph
The validator SHALL refuse a Graph with a connection that closes a loop, a connection between two input pins or two output pins, and a connection between two pins of one node. The refusal SHALL name the reason.

#### Scenario: Loop of two nodes
- **WHEN** a Graph connects node X to node Y and node Y back to node X
- **THEN** the validator refuses it with the reason `loop`

#### Scenario: Same direction
- **WHEN** a connection joins two output pins
- **THEN** the validator refuses it

#### Scenario: Same node
- **WHEN** a connection joins the output pin and an input pin of one node
- **THEN** the validator refuses it

### Requirement: Loop check across Graphs
The validator SHALL refuse a Graph that closes a loop through the Graph sensors of several Graphs, a Graph that reads its own Graph sensor included.

#### Scenario: Two Graphs read each other
- **WHEN** Graph A reads the Graph sensor of Graph B and the user applies Graph B with a source on the Graph sensor of Graph A
- **THEN** the validator refuses it with the reason `loop-across-graphs`

#### Scenario: Graph reads itself
- **WHEN** a Graph with a sensor end node has a source on its own Graph sensor
- **THEN** the validator refuses it

### Requirement: Graph file format
`graphs.json` SHALL hold `schema_version` (1) and `graphs`. A Graph SHALL have `id`, `name`, `enabled`, `sensor_id`, `nodes` and `connections`. A node SHALL have `id`, `kind`, `label`, `params`, `x` and `y`. A connection SHALL have `from` and `to`, each with `node` and `pin`. The output pin SHALL be named `out`.

#### Scenario: Example
- **WHEN** a Graph with a sensor end node has a constant node 1 with `value` 5 and a sensor end node 2
- **THEN** the file holds `{"id":"graph_1","name":"Five","enabled":true,"sensor_id":"graph_1","nodes":[...],"connections":[{"from":{"node":1,"pin":"out"},"to":{"node":2,"pin":"a"}}]}` with the two nodes

#### Scenario: Kind names
- **WHEN** a Graph is stored
- **THEN** `kind` is one of `sensor_source`, `alarm_event_source`, `constant`, `random`, `process`, `add`, `subtract`, `multiply`, `divide`, `minimum`, `maximum`, `average`, `logarithm`, `power`, `sine`, `cosine`, `tangent`, `square_root`, `round`, `limiter`, `table`, `less`, `less_equal`, `equal`, `greater_equal`, `greater`, `not_equal`, `and`, `or`, `not`, `switch`, `has_value`, `memory`, `persistent_storage`, `trigger`, `on_delay`, `off_delay`, `counter`, `peak`, `window_minimum`, `window_average`, `low_pass`, `trend`, `generator_linear`, `generator_sawtooth`, `generator_triangle`, `generator_sine`, `generator_rectangle`, `comment`, `sensor_end`, `response_end` (51 names)

#### Scenario: Input pin names
- **WHEN** a Graph is stored
- **THEN** the input pins are named `a` and `b` (two-input functions, comparisons, and, or), `a` (one-input kinds and end nodes), `s`, `a`, `b` (switch), `s`, `a` (memory, persistent storage), `count`, `enable`, `reset`, `period` (counter), `enable`, `reset`, `period` (generators)

#### Scenario: Parameter names
- **WHEN** a Graph is stored
- **THEN** `params` uses `sensor`; `event` and `source`; `value`; `min` and `max`; `name`; `decimals` and `mode` (`nearest`, `down`, `up`); `mode` (`linear`, `step`) and `pairs` (a list of [input, output]); `delay_s`; `span_s`; `span_s` and `damping`; `unit`; `text`, `width` and `height`; and for `response_end`: `kind` (`notification`, `sound`, `command`, `email`, `mqtt`, `poweroff`, `reboot`, `suspend`), `hold_s`, `repeat_s` (null for off), `min_interval_s`, and `title`, `text`, `urgency`; `sound`; `path`, `args`, `time_limit_s`; `to`, `subject`, `text`; `topic`, `payload`; `countdown_s`

### Requirement: Graph storage
The app SHALL store all Graphs in `graphs.json` in `$XDG_CONFIG_HOME/nicecooling/` with an atomic write. The app SHALL parse the file one Graph at a time and run the validator on each. A Graph that does not parse, has an unknown kind or fails the validator SHALL stay in the file with its stored text, run nothing and show the state "invalid". The other Graphs SHALL load. A file that is not a JSON list of Graphs, or has a higher schema version, SHALL stay unchanged and the app SHALL show an error.

#### Scenario: Restart
- **WHEN** the user applies a Graph and the app restarts
- **THEN** the Graph has the same nodes, connections and Graph sensor

#### Scenario: Invalid Graph at load
- **WHEN** one of 3 stored Graphs breaks a rule of the validator
- **THEN** the app evaluates 2 Graphs and lists the third as "invalid"

#### Scenario: Unknown kind
- **WHEN** one of 3 stored Graphs has a node of the kind `quantum`
- **THEN** the app evaluates 2 Graphs, lists the third as "invalid" and keeps its text when it writes the file

#### Scenario: Higher schema version
- **WHEN** `graphs.json` has `schema_version` 2 and the app knows 1
- **THEN** the app writes no Graph file and shows an error

### Requirement: Graph state storage
`graph-state.json` in `$XDG_DATA_HOME/nicecooling/` SHALL hold `schema_version` (1), `values` (the persistent storage nodes, keyed `<graph id>/<node id>`), `responses` (the same key, with `state` as `run` or `not_run` and `last_run_us`) and `pause` (`count`, `quiet_s`, `paused`). The app SHALL write it at most every 30 s when a value changed, at its stop and before a power call.

#### Scenario: Write rate
- **WHEN** a stored value changes every second for 60 s
- **THEN** the app writes the file at most 3 times in those 60 s

#### Scenario: Before a power call
- **WHEN** a poweroff response runs 5 s after the last write
- **THEN** the app writes the file before it calls logind

#### Scenario: Keys
- **WHEN** node 4 of `graph_2` is a persistent storage node that holds 7
- **THEN** `values` holds `"graph_2/4": 7`

### Requirement: Apply resets state
Apply of a Graph SHALL reset the state of its nodes except the stored values and the run state of nodes that keep their id and kind. The validator SHALL check the Graph again.

#### Scenario: Window after Apply
- **WHEN** the user changes a constant in a Graph with a 60 s average filter and applies it
- **THEN** the filter starts with an empty window

#### Scenario: Stored value kept
- **WHEN** the user changes another node of a Graph and applies it
- **THEN** the persistent storage node keeps its stored value

### Requirement: Graph export
The app SHALL export one Graph to one JSON file with `format` `nicecooling-graph`, `format_version` 1 and `graph` (`name`, `nodes` and `connections`, as in `graphs.json`). The file SHALL hold no `sensor_id`, no stored value, no run state and no password. The daemon SHALL have no part in it.

#### Scenario: Export file
- **WHEN** the user exports a Graph of 5 nodes
- **THEN** the file holds 5 nodes, the connections, the format name and the version 1

#### Scenario: No secret in the file
- **WHEN** the Graph has an e-mail response
- **THEN** the file holds the recipient, the subject and the text, and no SMTP password

### Requirement: Graph import
The app SHALL import a Graph from one file of at most 1 MiB. It SHALL give the Graph and its nodes new ids and add it disabled with no `sensor_id`. A sensor source that does not exist on this machine SHALL stay as an unbound source. A file with a higher format version or a Graph that fails the validator SHALL be refused with a message. The daemon SHALL have no part in it.

#### Scenario: Import twice
- **WHEN** the user imports the same file twice
- **THEN** two Graphs exist with different ids

#### Scenario: Unknown sensor
- **WHEN** the file has a source on `nct9999_temp7` and that sensor does not exist
- **THEN** the Graph imports and the node is unbound

#### Scenario: Imported response stays off
- **WHEN** the user imports a Graph with a command response
- **THEN** the Graph is disabled until the user enables it

#### Scenario: Version 2
- **WHEN** the file has `format_version` 2
- **THEN** the app refuses it with a message

### Requirement: Enable of a Graph with a sensor end node without a sensor
When the user enables a Graph with a sensor end node that has no `sensor_id`, the app SHALL call `Manager.CreateGraphSensor` first and store the returned id. When the daemon refuses, the Graph SHALL stay disabled and the app SHALL show the reason.

#### Scenario: Enable an imported Graph with a sensor end node
- **WHEN** the user enables an imported Graph with a sensor end node named "Hot water" with the unit `degC`
- **THEN** the app creates a Graph sensor `Hot water` and stores its id in `sensor_id`

#### Scenario: Limit reached
- **WHEN** 64 Graph sensors exist and the user enables an imported Graph with a sensor end node
- **THEN** the Graph stays disabled and the app shows that the limit is reached

### Requirement: Graph sensor in the daemon
The daemon SHALL keep Graph sensors in `state.json`: the key `graph_sensors` (a list of `id`, `name` and `unit`) and the key `graph_sensor_next`, which is 1 in a new state. `Manager.CreateGraphSensor(name, unit)` SHALL return the object path of a Sensor with the origin `graph` and the id `graph_<n>`. The counter SHALL never give an id twice. At most 64 SHALL exist. A name SHALL have up to 64 characters. The daemon SHALL run no Graph code.

#### Scenario: First sensor
- **WHEN** a new state has no Graph sensor
- **THEN** the first `CreateGraphSensor` returns `/io/github/cappyt/NiceCooling1/sensor/graph_1`

#### Scenario: Create
- **WHEN** the counter is 5
- **THEN** `CreateGraphSensor` returns `/io/github/cappyt/NiceCooling1/sensor/graph_5` and the counter is 6

#### Scenario: Id after remove
- **WHEN** the app removes `graph_5` and creates another
- **THEN** the new id is `graph_6`

#### Scenario: 65th sensor
- **WHEN** 64 Graph sensors exist and the app creates one more
- **THEN** the daemon refuses it with `InvalidArgument`

#### Scenario: Restart
- **WHEN** the daemon restarts
- **THEN** the Graph sensors exist with their names and units, no value, and the counter stays

### Requirement: Graph sensor units
The unit of a Graph sensor SHALL be one of `degC`, `rpm`, `%`, `V`, `A`, `W`, `l/h` and `mbar`. The `Kind` of the sensor SHALL come from the unit: `temperature` for `degC`, `fan` for `rpm`, `voltage` for `V`, `flow` for `l/h`, `pressure` for `mbar`, and `other` for `%`, `A` and `W`.

#### Scenario: Kind from unit
- **WHEN** the app creates a Graph sensor with the unit `mbar`
- **THEN** its `Kind` is `pressure`

#### Scenario: Unit not allowed
- **WHEN** the app creates a Graph sensor with the unit `bar`
- **THEN** the daemon refuses it with `InvalidArgument`

#### Scenario: Change of unit
- **WHEN** the app calls `SetDefinition("Water", "l/h")` on `graph_2`
- **THEN** the `Kind` becomes `flow` and the id stays `graph_2`

### Requirement: Graph sensor definition
`GraphSensor.SetDefinition(name, unit)` SHALL change the default name and the unit and keep the id. A name that the user set with `Sensor.SetName` SHALL still show. After it the daemon SHALL remove the object and add it again. A change of the unit SHALL fail with `InUse` while a Software sensor slot uses the sensor. `GraphSensor.Delete()` SHALL remove the sensor.

#### Scenario: Rename
- **WHEN** the app calls `SetDefinition("Hot water", "degC")` on `graph_2`
- **THEN** the id stays `graph_2`, the daemon emits `InterfacesRemoved` and `InterfacesAdded`, and every user keeps working

#### Scenario: User name wins
- **WHEN** the user named `graph_2` "Mine" with `Sensor.SetName` and the app calls `SetDefinition("Hot water", "degC")`
- **THEN** the sensor still shows "Mine" and its default name is "Hot water"

#### Scenario: Unit change in a slot
- **WHEN** slot 4 of a Leakshield holds `graph_2` and the app changes its unit from `degC` to `l/h`
- **THEN** the daemon answers `InUse`

#### Scenario: Unit change not in a slot
- **WHEN** no slot holds `graph_2` and the app changes its unit from `degC` to `l/h`
- **THEN** the `Kind` becomes `flow`

### Requirement: Graph sensor in use
The delete of a Graph sensor that a Curve, a sensor Profile rule or a Software sensor slot uses SHALL fail with `InUse` and the name of the user. The delete of any sensor that a Software sensor slot uses SHALL fail with `InUse`: a Graph sensor, a Derived sensor and a calibrated flow.

#### Scenario: Used by a Curve
- **WHEN** a Curve has `graph_2` as Input and the app calls `Delete` on it
- **THEN** the daemon answers `InUse` and names the Curve

#### Scenario: Derived sensor in a slot
- **WHEN** slot 5 holds `derived_1` and a client calls `DerivedSensor.Delete` on it
- **THEN** the daemon answers `InUse` and names the slot

#### Scenario: Loop with a calibrated flow in a slot
- **WHEN** slot 6 holds `loop_1_flow` and a client calls `Loop.Delete` on `loop_1`
- **THEN** the daemon answers `InUse` and names the slot

### Requirement: Graph sensor consumers
The daemon SHALL accept a Graph sensor as the Input of a Curve, as the source of a sensor Profile rule and in a Software sensor slot. It SHALL refuse a Graph sensor as the source of a Derived sensor and as the flow or water temperature sensor of a Loop, with `InvalidArgument`.

#### Scenario: Curve Input
- **WHEN** `SetCurve` names `graph_1` as Input
- **THEN** the daemon accepts it

#### Scenario: Derived source
- **WHEN** `CreateDerivedSensor` names `graph_1` as a source
- **THEN** the daemon refuses it with `InvalidArgument`

#### Scenario: Loop water temperature
- **WHEN** `SetWaterTemperature` names `graph_1`
- **THEN** the daemon refuses it with `InvalidArgument`

### Requirement: Graph sensor values
`Manager.SetGraphSensorValues(a(sbd))` SHALL take one entry per Graph sensor: the id, "has a value" and the value. The app SHALL send it once per second. An entry with "has a value" false SHALL make the sensor lose its value at once. A number that is not finite SHALL fail the whole call with `InvalidArgument`. An entry whose id is not the id of a Graph sensor SHALL be skipped.

#### Scenario: Values
- **WHEN** the app sends (`graph_1`, true, 55.0) and (`graph_2`, false, 0.0)
- **THEN** the value table holds 55.0 for `graph_1` and no value for `graph_2` in the next tick

#### Scenario: Number not finite
- **WHEN** an entry holds a value that is not finite
- **THEN** the daemon refuses the call with `InvalidArgument` and changes no value

#### Scenario: Unknown id
- **WHEN** an entry names a sensor that was removed
- **THEN** the daemon skips it and accepts the others

#### Scenario: Entry for a measured sensor
- **WHEN** an entry names `nct6798_temp1`
- **THEN** the daemon skips it and the sensor keeps its measured value

### Requirement: Graph sensors at start
At its start and at each new snapshot of the daemon, the app SHALL compare the Graphs with a sensor end node with the Graph sensors of the daemon. It SHALL create a missing one and store the new id in the Graph. It SHALL leave a Graph sensor that none of its Graphs holds. The Graphs screen SHALL list such a sensor with a remove command. An app in a session that is not active SHALL send no Graph sensor values.

#### Scenario: Missing sensor
- **WHEN** a Graph holds `graph_3` and the daemon has no such sensor
- **THEN** the app creates a Graph sensor with the name and unit of the Graph and stores its id

#### Scenario: Sensor of another user
- **WHEN** the daemon holds `graph_7` and none of the Graphs holds it
- **THEN** the app leaves it and the Graphs screen lists it with "Remove"

#### Scenario: Inactive session
- **WHEN** the session of the app is not active
- **THEN** the app sends no Graph sensor values

### Requirement: Graph sensor without the app
A Graph sensor value older than 3 s SHALL count as no value, as for every sensor. So a Graph sensor SHALL have no value when the app does not run, stops or hangs.

#### Scenario: App closed
- **WHEN** the app stops and 3 s pass
- **THEN** every Graph sensor has no value and a Curve on one has a Lost Input

#### Scenario: Before login
- **WHEN** the daemon starts at boot and no user is logged in
- **THEN** every Graph sensor has no value

### Requirement: Graph sensor authorization
The four members `CreateGraphSensor`, `SetDefinition`, `Delete` and `SetGraphSensorValues` SHALL need the polkit action `io.github.cappyt.NiceCooling1.write`. The daemon SHALL ask polkit at every call. The app SHALL call `SetGraphSensorValues` on its own call path without the interactive flag. After a denial the app SHALL stop for 60 s and show that it has no access.

#### Scenario: Wheel member
- **WHEN** a `wheel` member in the active session sends the values
- **THEN** every call succeeds with no dialog

#### Scenario: Other user
- **WHEN** a user outside `wheel` sends the values
- **THEN** the call fails, the app sends nothing for 60 s and shows that it has no access

#### Scenario: After the pause
- **WHEN** 60 s pass after a denial
- **THEN** the app tries the call again

#### Scenario: Other writes
- **WHEN** the app calls `CreateGraphSensor`
- **THEN** the call carries the interactive flag

### Requirement: Graph sensor as a source
A Graph sensor SHALL be a normal sensor with the origin `graph`. It SHALL serve as the Input of a Curve, as the source of a sensor Profile rule, as a Software sensor of a Leakshield, as a series of the app history, as a topic of MQTT and as the source of a Graph.

#### Scenario: Curve Input
- **WHEN** a Curve has a Graph sensor as Input and the app sends 55
- **THEN** the Curve computes its Duty for the Input 55 in the next tick

#### Scenario: Profile rule
- **WHEN** a sensor Profile rule has a Graph sensor as source with the comparison `gt` 50 and the value is 55
- **THEN** the rule holds after its hold time

#### Scenario: Software sensor
- **WHEN** a Software sensor slot takes a Graph sensor
- **THEN** the Leakshield Feed message carries its value

### Requirement: Derived sensors unchanged
This change SHALL not convert, change or remove a Derived sensor. Their methods and ids SHALL stay as `add-nicecooling-v1` defines them.

#### Scenario: Update
- **WHEN** the daemon starts with 3 Derived sensors stored
- **THEN** it still has 3 Derived sensors with the same ids and no Graph is created

#### Scenario: Methods
- **WHEN** a client calls `CreateDerivedSensor`
- **THEN** the method works as before

### Requirement: Curve editor note
When the user picks a Graph sensor as the Input of a Curve, the Curve editor SHALL show the text "A Graph sensor has no value while the app does not run. The Channel then runs at 100%."

#### Scenario: Pick a Graph sensor
- **WHEN** the user picks `graph_1` as Input
- **THEN** the editor shows the text

#### Scenario: Pick a measured sensor
- **WHEN** the user picks `nct6798_temp1` as Input
- **THEN** the editor shows no such text

### Requirement: Graphs list screen
The app SHALL show a screen "Graphs" in the group "Control". It SHALL list each Graph with its name, the kind of its end node, its state and its live result. It SHALL offer "New sensor", "New response" and "Import". Each row SHALL offer an enable switch, "Edit", "Export" and "Delete" with a confirmation.

#### Scenario: Live result of a Graph with a sensor end node
- **WHEN** a Graph with a sensor end node gives 46.2 in degrees Celsius
- **THEN** its row shows "46.2 °C" and its state "Running"

#### Scenario: Live result of a Graph with a response end node
- **WHEN** a response end node is in the state "run"
- **THEN** its row shows "Fired" and the time of its last run

#### Scenario: Delete a used sensor
- **WHEN** the user deletes a Graph whose Graph sensor a Curve uses
- **THEN** the daemon answers `InUse`, the Graph stays and the app shows the name of the Curve

### Requirement: Sensors screen
The Sensors screen SHALL list the Graph sensors in a group "Graph sensors". It SHALL open the Graph editor on the Graph of a Graph sensor. The Derived sensor dialog SHALL stay.

#### Scenario: Open a Graph sensor
- **WHEN** the user opens a Graph sensor from the Sensors screen
- **THEN** the app opens the Graph editor on its Graph

#### Scenario: Derived sensor dialog
- **WHEN** the user presses "New Derived sensor"
- **THEN** the existing dialog opens

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
The editor SHALL show a node list grouped by family: sources, math, logic, state and time, filters, generators and other. It SHALL have a search field. A new node SHALL appear at the centre of the view by double click or by drag onto the canvas. The list SHALL disable the kind of the end nodes because the Graph has its end node.

#### Scenario: Search
- **WHEN** the user types "delay"
- **THEN** the list shows the on delay and off delay nodes

#### Scenario: Add by double click
- **WHEN** the user double-clicks "Constant"
- **THEN** the canvas gets a constant node at the centre of the view

#### Scenario: Kind count
- **WHEN** the user opens the list
- **THEN** it holds 51 kinds in total, 2 of them end nodes

### Requirement: Editor live values
The editor SHALL show the value of each node of the applied Graph on the node, with `--` for no value. It SHALL read the values from the evaluator of the app once per second while it is open. A node that exists only in the draft SHALL show no value until Apply.

#### Scenario: Live value
- **WHEN** a node gives 46.2
- **THEN** the node shows "46.2" within 2 seconds

#### Scenario: No value
- **WHEN** a node has no value
- **THEN** the node shows "--"

#### Scenario: New node
- **WHEN** the user adds a constant node to the draft
- **THEN** the node shows "--" until Apply

### Requirement: Editor Try
The editor SHALL offer "Try" for a draft. A sensor source node SHALL offer a test value and a test time of 1 to 600 s, default 60 s. A source with no test value SHALL read its live value. Try SHALL evaluate the draft in the app, show the node values and the banner "Try running, nothing is published", and stop at the end of the time or on "Stop". A try SHALL send no Graph sensor value and SHALL run no response.

#### Scenario: Run a try
- **WHEN** the user gives the source 80 for 30 s and presses "Try"
- **THEN** the nodes show the values of the try and the banner is visible

#### Scenario: Source with no test value
- **WHEN** a draft has two sources, one with the test value 80 and one with none, and the second sensor reads 20
- **THEN** the second source gives 20

#### Scenario: Nothing published
- **WHEN** a try runs on a Graph with a sensor end node whose Graph sensor a Curve uses
- **THEN** the app still sends the value of the applied Graph

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
The editor SHALL work on a draft. The app SHALL store the Graph only at "Apply". The button SHALL be off when nothing changed or when the draft breaks a rule of the validator, and the editor SHALL show the first broken rule. Apply of a Graph with a sensor end node SHALL create or rename its Graph sensor first and SHALL fail when the daemon refuses. Leaving with an unapplied draft SHALL ask: Apply, Discard or Cancel.

#### Scenario: Apply
- **WHEN** the user presses "Apply" on a valid draft
- **THEN** the app stores the Graph and the button turns off

#### Scenario: Broken rule
- **WHEN** the draft has two end nodes
- **THEN** "Apply" is off and the editor shows "A Graph has one end node"

#### Scenario: Leave with a draft
- **WHEN** the user closes the editor with an unapplied change
- **THEN** the app asks Apply, Discard or Cancel

#### Scenario: Daemon refuses
- **WHEN** the daemon refuses the Graph sensor at Apply with `InvalidArgument`
- **THEN** the editor keeps the draft, stores nothing and shows the reason

#### Scenario: Change the kind of the end node of a used Graph sensor
- **WHEN** the user changes a Graph with a sensor end node into a Graph with a response end node while a Curve uses its Graph sensor
- **THEN** Apply fails with `InUse` and the app stores nothing

### Requirement: Editor states
The editor SHALL show these states. Daemon offline: a banner, dim values, the draft stays and "Apply" is off for a Graph with a sensor end node. Read error: a message with "Retry"; a Graph file that cannot be read opens no editor, and a failed value read shows dim `--`. Unbound source: the node shows a warning icon, the stored sensor id and "Sensor not found", and the user can choose another sensor.

#### Scenario: Daemon goes offline
- **WHEN** the daemon becomes unreachable while the editor is open
- **THEN** the editor shows the banner, keeps the draft and turns "Apply" off for a Graph with a sensor end node

#### Scenario: Graph file unreadable
- **WHEN** the app cannot read `graphs.json`
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

