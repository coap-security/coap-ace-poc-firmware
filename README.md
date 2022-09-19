CoAP/ACE PoC: Firmware
======================

This repository contains the firmware part of the CoAP/ACE proof-of-concept implementation.
The firmware is written in Rust,
and designed to run on [nRF52-DK] hardware based on the [S132 softdevice]
(but is easy to adjust to other nRF devices).

[nRF52-DK]: https://www.nordicsemi.com/Products/Development-hardware/nRF52-DK
[S132 softdevice]: https://www.nordicsemi.com/Products/Development-software/s132/

What this does
--------------

With this firmware,
the device it is running on simulates a simple network enabled sensor --
it reports the temperature it measures over a radio interface to authorized users,
and allows some users to alter the identification LEDs.

The technology stack it demonstrates by this is

* an [ACE (RFC9200)] Resource Server (RS) -- this limits the interactions of users according to an authorization server's decisions
* the [ACE OSCORE profile (RFC9203)] -- this connects the ACE framework with OSCORE, and generates OSCORE keys
* [OSCORE (RFC8613)] -- this secures communication with symmetric keys, independent of the precise transport mechanism used
* [CoAP (RFC7252)] -- this gives a compact and versatile application protocol with flexible forwarding options
* [CoAP-over-GATT (`draft-amsuess-core-coap-over-gatt-02`)] -- this allows transporting CoAP over Bluetooth Low Energy (BLE) without the need to set up a Bluetooth IP network

[ACE (RFC9200)]: https://www.rfc-editor.org/rfc/rfc9200.html
[ACE OSCORE profile (RFC9203)]: https://www.rfc-editor.org/rfc/rfc9203.html
[OSCORE (RFC8613)]: https://www.rfc-editor.org/rfc/rfc8613.html
[CoAP (RFC7252)]: https://www.rfc-editor.org/rfc/rfc7252.html

Quick start: Running the proof-of-concept demo
----------------------------------------------

* Obtain an [nRF52-DK] device; connect it via USB to a computer and move its power switch to the "on" position.
* Download the latest build of this firmware from TBD as `TBD.hex`.
* Copy the file `TBD.hex` onto the "JLINK" USB drive that has appeared on your computer.
* Restart the board by moving the board's power switch to "off" and back to "on".
  <!-- Merely pressing the BOOT/RESET button is insufficient. -->
* Direct a Bluetooth capable's cellphone web browser (Chrome or Chromium) to TBD. Follow the login instructions, picking either the role of the "junior" or the "senior".

  You may also install the mobile application through the browser's "Install app" button.

* Use the web application's controls to read the device's temperature,
  or to alter its identification LEDs ("senior" only).

  For illustration purposes, the web application is not made aware of the permission levels,
  and unauthorized control attempts will fail.

The workings -- getting to know the components
----------------------------------------------

* Instructions on how to build the firmware,
  as well as how to alter or extend it,
  can be found in the firmware's TBD documentation.

  That also contains further references for the components used inside the firmware.

* The web application is built from source at TBD
  and documented at TBD.
  The web application is built into a static web site,
  which can be served by any modern web server.

* Backing the authentication is an ACE Authorization Server (AS).
  Its setup and operatioins are described on TBD.

License
-------

The software is proprietary and confidential until properly released;
it is expected to be released under 3-clause BSD terms.

Note that upon linking against the softdevice,
the softdevice's license terms become part of what governs the use of the resulting program.
