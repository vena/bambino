# Chapter 4: Toolhead, Thermal, Motion & Climate Systems

---

### 4.1 Network Boundary & Interface Parameters [REF-MOTO-GCODE]

All toolhead positioning, thermal profiles, and climate adjustments are executed on the physical printer through standard G-code instructions. To dispatch these commands, the raw G-code strings are encapsulated inside the JSON `gcode_line` control wrapper defined in `[REF-MQTT-LIFECYCLE]` and routed over MQTTS Port `8883` to the publish topic:

`device/{serial_number}/request`

Multiple G-code commands can be executed sequentially by separating distinct G-code lines with newline characters (`\n`) within a single serialized JSON parameter string.

---

### 4.2 State Telemetry Decoding [REF-THER-DECODE]

Thermal, configuration, and structural climate performance data are evaluated using specific mathematical unpacking algorithms applied to the raw state telemetry fields mapped in `[REF-MQTT-ENV]`.

#### Temperature Decoding [REF-THER-DECODE]
For models supporting advanced thermal monitoring (such as the heatbed, active chamber heaters, and dual-nozzle extruders on H2D platforms), actual and target temperatures are packed and transmitted as a single composite 32-bit unsigned integer to conserve network bandwidth and indicate active heating states.

The system evaluates the raw telemetry field value to determine the encoding state:
*   **Direct Value (Value $\le 500$)**: The reported integer value represents the raw actual temperature directly in degrees Celsius. The target temperature is implicitly $0^\circ\text{C}$ (or is tracked via separate target registers).
*   **Composite Packed Value (Value $> 500$)**: When a heater is active and a non-zero target temperature is set, the field value exceeds 500. The field is transmitted as a single composite 32-bit unsigned integer containing both the target and actual temperatures.

The system must unpack these composite integers dynamically:

*   **Target Temperature**: Extracted by shifting the composite integer right by 16 bits:

    $$\text{Target} = \text{packed\_val} \gg 16$$

*   **Actual Temperature**: Extracted from the lower 16 bits of the composite integer using a bitwise AND mask:

    $$\text{Actual} = \text{packed\_val} \ \& \ 0\text{xFFFF}$$

##### Bed Temperature Wire Paths
Bed temperature telemetry arrives via different wire paths depending on the hardware generation:
*   **New-gen models (H2, P2, X2 series)**: Bed temperature is reported at `device.bed.info.temp` as a composite-packed integer (values > 500 encode target and actual). The `device.bed.state` field indicates heater state (e.g. `2` = heating).
*   **Old-gen models (P1, A1, X1 series)**: Bed temperature is reported via two direct fields: `print.bed_temper` (actual temperature as a float) and `print.bed_target_temper` (target temperature as a float). These are not composite-packed.

Clients should check `device.bed` first (including the nested `print.device.bed` path used in pushall responses on new-gen models), then fall back to the old-gen direct fields.

##### Case Study: Bed Temperature Composite Encoding
When the heatbed target temperature and actual temperature are both set to 100°C, the telemetry channel transmits the packed decimal integer `6553700` (which is hex `0x00640064`).
*   $\text{Target} = 0\text{x}00640064 \gg 16 = 0\text{x}0064 = 100^\circ\text{C}$
*   $\text{Actual} = 0\text{x}00640064 \ \& \ 0\text{xFFFF} = 0\text{x}0064 = 100^\circ\text{C}$

##### Chamber Temperature Target & Heater State Encoding (H2D Series)
On dual-extruder platforms equipped with active chamber heaters (such as the `H2D` series), this composite encoding applies to the chamber temperature parameters (`chamber_temper` and `info.temp` fields). The parser must evaluate the value dynamically:
*   **Value $> 500$**: The chamber heater is actively enabled. The value is composite-encoded representing $(\text{target} \times 65536) + \text{current}$.
*   **Value $\le 500$**: The chamber heater is inactive. The telemetry reports direct Celsius temperature, and the target is implicitly `0`.

##### Chamber Temperature Sensor Availability Constraints
Only specific hardware lines (specifically the `X1C`, `X1E`, `X2D`, `P2S`, `H2C`, `H2D`, `H2D Pro`, and `H2S`) are equipped with physical chamber temperature sensors. For open-frame or entry-level models (specifically the `P1P`, `P1S`, `A1`, `A1 Mini`, and `A2L` series), no physical chamber temperature sensor exists on the toolhead or enclosure bus. Consequently, any `chamber_temper` or `ctc.info.temp` values reported in their telemetry status streams are meaningless static or junk values and must be ignored by state tracking systems.

##### Temperature Operating Limits [REF-THER-LIMITS]
Maximum safe operating temperatures per model, sourced from OrcaSlicer printer configuration files and Bambu Lab product specifications (bambulab.com/en/compare). These limits represent the hardware-enforced maximum values accepted by the firmware for `M104` (nozzle), `M140` (bed), and `M141` (chamber) G-code commands.

| Model | Nozzle Max (°C) | Bed Max (°C) | Chamber Max (°C) | Notes |
| :--- | :--- | :--- | :--- | :--- |
| A1 | 300 | 100 | — | |
| A1 Mini | 300 | 80 | — | |
| A2L | 300 | 80 | — | |
| P1P | 300 | 100 | — | |
| P1S | 300 | 100 | — | |
| P2S | 300 | 110 | — | Has chamber sensor but no active PTC heater |
| X1C | 300 | 120 | — | Bed limit 110°C on 220V, 120°C on 110V |
| X1E | 320 | 110 | 60 | |
| X2D | 300 | 120 | 65 | |
| H2S | 350 | 120 | 65 | |
| H2D | 350 | 120 | 65 | |
| H2D Pro | 350 | 120 | 65 | |
| H2C | 350 | 120 | 65 | |

#### Dual-Extruder Temperature Routing
On dual-extruder IDEX architectures (such as the H2D), standard telemetry keys are mapped as follows:
*   `nozzle_temper`: Mirrors the actual temperature of the left nozzle (Extruder 1 / Deputy).
*   `nozzle_target_temper`: Mirrors the target temperature of the right nozzle (Extruder 0 / Main).
*   The actual temperature of the right nozzle and target temperature of the left nozzle must be parsed from the `device.extruder.info` array:
    *   Index `0` represents the Right/Main nozzle.
    *   Index `1` represents the Left/Deputy nozzle.

##### Extruder Collection State Bitmask
The `device.extruder.state` field is a 32-bit unsigned integer encoding the physical extruder topology:
*   **Bits 0–3 (low nibble)**: Extruder count (e.g. `2` for dual-nozzle IDEX).
*   **Bits 4–7**: Active extruder index (`0` = Right/Main, `1` = Left/Deputy).

To extract: `extruder_count = state & 0xF`, `active_index = (state >> 4) & 0xF`.

#### Nozzle & Carriage Kinematics
Carriage structural parameters are parsed from `device.nozzle.info` (refer to `[REF-NOZZLE-KEYS]` for raw key mappings). 
*   **Standard & Dual-Nozzle Systems (X1, P1, A1, P2S, H2D, H2D Pro, H2S)**: Actively evaluate extruder carriage IDs `0` (Right/Main) and `1` (Left/Deputy) to track physical temperature offsets and toolhead alignment during IDEX printing operations.
*   **Vortek Tool Changer Systems (H2C)**: Interpret nozzle IDs `16` to `21` strictly as physical hotends resting inside the passive hotend storage rack. The `"stat"` parameter for these rack slots indicates tool status (such as `0` for empty, or bitmask states representing presence, alignment, and locking confirmations).

#### Fan Speed Telemetry Step-to-Percentage Conversions [REF-CLIM-FANS]
The speed values for the cooling fans (excluding the X2D secondary auxiliary fan) are reported on the MQTTS status channel as discrete step integers on a scale of `0` to `15` (mapped in `[REF-CLIM-FANS]`). Receiving applications must map these steps ($S$) to standard percentage values:

$$\text{Percentage} = \text{Round}\left(S \times \frac{100}{15}\right) \approx \text{Round}(S \times 6.67)$$

*   **Secondary Auxiliary Fan (Right Auxiliary Fan on X2D)**: Mapped to `device.airduct.parts` with `id: 160`. The `"state"` parameter of this part object represents the actual speed directly as an integer percentage value ($0$ to $100$) and does not require step conversion.

##### Fan Oscillation Telemetry Artifacts
Due to discrete 0–15 PWM-step quantization within the physical fan controller, the reported fan speed value in telemetry may oscillate rapidly between adjacent integer steps when trying to maintain fractional target speeds. Downstream systems parsing this status stream must handle this oscillation to prevent interface flickering or false fan speed alert triggers.

---

### 4.3 Over-the-Wire Control G-Code Streams

The following raw G-code command sequences are serialized and transmitted within the `"param"` field of the `gcode_line` JSON envelope defined in `[REF-MQTT-LIFECYCLE]`.

#### Manual Bed and Nozzle Temperature Targets
Sets the target temperature of the heatbed to 60°C:
```gcode
M140 S60
```
Sets the target temperature of the Right/Main hotend (T0) to 220°C:
```gcode
M104 T0 S220
```

#### Manual Active Chamber Temperature Target
Sets the target temperature of the active chamber heating loop to 45°C (supported on enclosed models equipped with active PTC chamber heaters: `X1E`, `X2D`, `H2S`, `H2D`, `H2D Pro`, `H2C`). Note: `P2S` has a chamber temperature sensor and airduct-based heat retention but no dedicated PTC heater element — `M141` is silently ignored by P2S firmware:
```gcode
M141 S45
```

#### Manual Fan Speed Controls
Configures fan PWM duty cycles on a standard scale of `0` (off) to `255` (maximum speed).

Sets the part cooling fan speed to 100%:
```gcode
M106 P1 S255
```
Sets the primary auxiliary fan (left side) speed to approximately 50%:
```gcode
M106 P2 S128
```
Sets the chamber exhaust/filtration fan speed to 100%:
```gcode
M106 P3 S255
```
Sets the secondary auxiliary fan (right-side fan on the X2D) speed to 100%:
```gcode
M106 P10 S255
```

#### Manual Relative Axis Movement
Performs manual positioning on the Z-axis. To ensure safe execution during an `IDLE` state, motion blocks wrap relative commands with travel limit registrations:
```gcode
M211 S1
M1002 push_ref_mode
G91
G0 Z10.00 F3000
G90
M1002 pop_ref_mode
```
*   `M211 S1`: Re-enables software travel limits to protect against physical endstop crashes.
*   `M1002 push_ref_mode` / `pop_ref_mode`: Isolates and restores coordinate references to prevent frame shifting.
*   `G91` / `G90`: Switches to relative positioning for the move, then restores absolute mode.

#### Manual Material Extrusion [REF-GCODE-EXTRUDE]
Enables relative extruder positioning and commands the active extruder drive gear to feed 10.0mm of filament at a feedrate of 900mm/min:
```gcode
M83
G0 E10.00 F900
```

#### Active RFID Tag Scan [REF-GCODE-RFID]
Instructs the physical AMS slot corresponding to the global tray index to execute an active feed and RFID identification scan:
```gcode
M620 R{global_tray_index}
```

#### LED, Airduct, and Buzzer Systems
The high-level JSON command envelopes for controlling enclosure lighting, airduct directional dampers, and buzzer alarms (`ledctrl`, `set_airduct`, `print_option`, and `buzzer_ctrl`) are transmitted directly via the MQTTS API schemas documented in `[REF-MQTT-LIFECYCLE]`.

*   **LED Systems**: Carriages and enclosures utilize dual physical light strips (`chamber_light` and `chamber_light2`) to provide illumination for camera-exposure and structural monitoring.
*   **Airduct Climate Systems**: The damper actuators physically shift to redirect airflow. ModeId `0` (cooling) closes internal recirculation dampers to route hot air out through the filtration exhaust, ModeId `1` (heating) closes exhaust flaps to seal the enclosure and leverage heat from the build plate or chamber heater, and ModeId `2` (laser) configures airflow for laser engraving module operation. The printer reports available modes via `device.airduct.modeList` (array of `{"modeId": N}` entries) and the current mode via `device.airduct.modeCur`. Supported on: `H2S`, `H2D`, `H2D Pro`, `H2C`, `P2S`, `X2D` (confirmed by pybambu `Features.AIRDUCT_MODE` and `AIRDUCT_MODES` constant).
*   **Prompt Sound Notifications**: Configures onboard speaker prompt sounds during user-facing events. Supported on: `A1`, `A1 Mini`, `A2L` (confirmed by Bambu Studio profiles `support_prompt_sound`).
*   **Buzzer Alerting Systems**: The physical fire alarm buzzer module operates under strict safety limits, allowing silent, triggered alarm, or attention beeping states depending on firmware events. Supported on: `H2S`, `H2D`, `H2D Pro`, `H2C` (confirmed by pybambu `Features.FIRE_ALARM_BUZZER`).

#### External Tool Mount Telemetry (`device.ext_tool`)
On models supporting laser engraving or cutting attachments, the `device.ext_tool` object reports the current state of the externally mounted tool:
*   `mount`: Mount detection state (`0` = not mounted, `1` = mounted).
*   `type`: Tool type code string identifying the mounted accessory (`"LB00"` = 10W laser module, `"LB01"` = 40W laser module, `"CP00"` = cutting/plotting module).
*   `calib`: Calibration state indicator.
*   `low_prec`: Low-precision mode flag (boolean).
*   `th_temp`: Thermal head temperature of the mounted tool.
*   `mount_3d`: 3D print head mount state (tracks whether the standard FDM printhead is installed alongside or instead of the external tool).

---

### 4.4 Mechanical & Firmware Quirks

#### Axis Homing State Detection [REF-MOTO-HOME]
The per-axis homed state is reported via `home_flag` bits 0–2 in MQTT telemetry (see [REF-HOMEFLAG] for the full bitmask). The firmware does **not** reject motion gcode (`G0`, `G1`, `G91`) when axes are unhomed, and there is no magnitude-based limiting either — relative moves up to 20mm (tested on a P1S, both the bed-on-Z axis and the toolhead X/Y axes) execute the full requested distance without a valid coordinate frame. Homing enforcement exists only at the UI/slicer layer (touchscreen, OrcaSlicer), which checks `home_flag` client-side before allowing a jog — the firmware provides no backstop. Clients must check homed state before dispatching motion commands and block or warn at the application layer. During a `G28` homing sequence, `mc_print_sub_stage` transitions `0 → 1` (homing in progress) and back to `0` (homing complete) — observed via wire capture on a P1S. This field is **not homing-exclusive**: Bambuddy's independent reverse-engineering (`backend/app/services/bambu_mqtt.py`) documents the same field as a filament-change step indicator, used by OrcaSlicer/BambuStudio to track progress during filament load/unload. BambuStudio's own source (`DeviceManager.cpp`) only stores the raw int with no semantic dispatch, offering no corroboration either way. Treat a `0 → 1 → 0` transition as evidence of homing, not proof — corroborate against `home_flag` bits 0–2 (unhomed → homed) when disambiguating from an unrelated filament-change event matters. **Transient, not sustained**: a P1S probe run (`bambino-cli probe -t home_axes_with_busy_check`, run against a printer already several seconds into a UI-triggered `G28`) observed `mc_print_sub_stage` already back at its rest value while `home_flag` still showed all three axes unhomed — i.e. the `0 → 1` pulse occurs near the *start* of the homing cycle and reverts well before the ~20–40s cycle completes; it does not stay `1` for the cycle's duration. Do not gate "is currently homing" logic on observing `mc_print_sub_stage == 1` at an arbitrary later point — a client connecting partway through an externally-triggered home will likely miss the pulse entirely. `home_flag` (not all set, for the cycle's full duration) is the only field safe to use for that purpose. **Homing duration depends on physical starting position, not command origin**: two `bambino-cli probe -t home_axes_with_busy_check` runs measured 24s and 72s respectively — the 72s run had the bed positioned further from home, requiring more travel. Phase 8's self-triggered G28 data (P1S, n=6) was bounded at ~46s from typical starting positions; a printer with the bed near its lower limit can take considerably longer. 72s nearly halves the margin against `wait_for_homing()`'s 90s internal cap regardless of whether the command originated from this library or the touchscreen. If consumers may use `wait_for_homing()` from extreme bed positions, exposing `HOMING_WAIT_TIMEOUT_SECS` as a configurable parameter would be prudent.

#### MQTT-Native Motion Commands on Newer Models (Unverified) [REF-MOTO-MQTTCTRL]
Models that advertise support via the `fun` capability bitmask can dispatch homing and jogging as structured JSON commands instead of raw G-code: `{"print":{"command":"back_to_center","sequence_id":...}}` for homing (capability bit 32), and `{"print":{"command":"xyz_ctrl","axis":"X"|"Y"|"Z","dir":1|-1,"mode":0|1,"sequence_id":...}}` for jogging (capability bit 38). Source: Bambu Lab's official BambuStudio slicer (`DevAxisCtrl.cpp`) — not independently verified against any printer via wire capture. The `fun` field is absent entirely from P1/A1 ESP32-RTOS telemetry, so these commands are presumed unsupported on those models, but this is inferred from capability-flag absence, not confirmed by testing. Whether the firmware applies different homed-state or safety enforcement to this path versus raw G-code is unknown.

#### Z-Axis Homing Crash Hazards (Bed-on-Z vs. Bed-Slinger)
*   **Bed-on-Z Models (X1, P1, H2, P2S series)**: The build plate moves down on the Z-axis to increase nozzle clearance. Manual homing **must strictly be dispatched as a bare `G28` command**. This initiates the safe factory sequence: parking the XY toolhead prior to raising the Z bed. Transmitting restricted axis constraints (such as `G28 Z`) bypasses this safety check, causing the bed to drive immediately upward. If the toolhead is positioned over the build area, this results in a high-force nozzle collision with the plate.
*   **Bed-Slingers (A1, A1 Mini)**: The Z-axis controls toolhead height, not build plate position. Movement vectors are inverted (e.g., `G1 Z-10` drives the hotend down toward the print surface). Travel macros must evaluate the active hardware model to adjust movement directions and prevent build plate gouging.

#### No Soft-Limit Enforcement at Physical Travel End (P1S)
Even when fully homed, commanding further travel in the direction of a physical limit that has already been reached (e.g., continuing to jog the bed down on a bed-on-Z model after it has bottomed out) does not halt or reject the motion command. The firmware does not detect or prevent this — the motor simply grinds against the physical stop. Homed state alone does not imply the requested relative move is within safe physical travel range.

#### Physical Safety Fan Controls Override Lock
To protect against thermal creep, heatbreak clogging, or premature filament melting inside the cold zone of the extruder assembly, the motion-controller board enforces strict thermal-switch overrides. Manual speed overrides (`M106`) targeting the hotend fan (`heatbreak_fan`) are ignored by the queue during active print jobs or whenever the hotend temperature is 50°C or higher.
