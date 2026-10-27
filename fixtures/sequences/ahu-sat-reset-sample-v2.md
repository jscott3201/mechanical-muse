# Sequence of Operations: AHU-1 Supply Air Temperature Setpoint (Revision B)

Sample document written for Mechanical Muse to exercise "bring your own sequence". It is not
taken from any standard or project.

Revision B: the dehumidification thresholds in 3.4 are raised to suit this building, whose
return air normally runs at 55 to 65 % relative humidity. Everything else is unchanged.

## 1. General

AHU-1 serves an open-plan office floor. The building automation system calculates the discharge
air temperature setpoint described here and sends it to the AHU's cooling coil loop once a minute.

## 2. Inputs

- AHU enable (on/off).
- Outdoor air temperature, °F.
- Return air relative humidity, %.

## 3. Setpoint calculation

3.1 While the AHU is off, the setpoint is 55 °F.

3.2 When the AHU starts, hold the setpoint at 55 °F for the first 10 minutes so the space is
pulled down quickly. The 10 minutes start again every time the AHU starts.

3.3 After that, reset the setpoint from outdoor air temperature: 55 °F when it is 70 °F or warmer
outside, 65 °F when it is 60 °F or colder, and in a straight line between those points.

3.4 Dehumidification: if return air humidity has stayed above 70 % for 5 minutes, use 55 °F
regardless of outdoor air temperature, until humidity drops below 65 %.

## 4. Outputs

- Discharge air temperature setpoint, °F.
- Dehumidification active (on/off), for the operator's graphics.
