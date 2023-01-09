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
[CoAP-over-GATT (`draft-amsuess-core-coap-over-gatt-02`)]: https://www.ietf.org/archive/id/draft-amsuess-core-coap-over-gatt-02.html

Quick start: Running the proof-of-concept demo
----------------------------------------------

* Obtain an [nRF52-DK] device; connect it via USB to a computer and move its power switch to the "on" position.
* Download the latest build of this firmware [from the build site] as `coap-ace-poc-firmware-.d??.hex`.
  When using just a single nRF52-DK, pick any of numbered images.
  When using multiple devices, pick distinct ones, and consider labelling the devices accordingly.
* Copy the file `coap-ace-poc-firmware-d??.hex` onto the "JLINK" USB drive that has appeared on your computer.
  Once copying is done, the device will restart,
  show two LEDs indicating the application's readiness,
  and USB drive will reappear without the file.
* Direct a Bluetooth capable's cellphone web browser (Chrome or Chromium) to [the corresponding web app].
  Follow the login instructions, picking either the role of the "junior" or the "senior".
  Note that in the course of the login, the BLE connection is severed and needs to be reestablished.

  You may also install the mobile application through the browser's "Install app" button.

* Use the web application's controls to read the device's temperature,
  to find the device (making its LEDs spin briefly),
  or to alter its identification LEDs ("senior" only).

  For illustration purposes, the web application is not made aware of the permission levels,
  and unauthorized control attempts will fail.

[from the build site]: https://oscore.gitlab.io/coap-ace-poc-firmware/
[the corresponding web app]: https://oscore.gitlab.io/coap-ace-poc-webapp/

The workings -- getting to know the components
----------------------------------------------

* Instructions on how to build the firmware,
  as well as how to alter or extend it,
  can be found in the [firmware's documentation].

  That also contains further references for the components used inside the firmware.

* The web application is built from source at https://gitlab.com/oscore/coap-ace-poc-webapp/
  and [documented there as well].
  The web application is built into a static web site,
  which can be served by any modern web server.

* Backing the authentication is an ACE Authorization Server (AS).
  Its setup and operatioins are described on TBD.


[firmware's documentation]: https://oscore.gitlab.io/coap-ace-poc-firmware/doc/coap_ace_poc_firmware/
[documented there as well]: https://oscore.gitlab.io/coap-ace-poc-webapp/doc/coap_ace_poc_webapp/

License
-------

Copyright 2022 EDF. This software was developed in collaboration with Christian Amsüss.

This software is published under the terms of the BSD-3-Clause license
as detailed in [LICENSE file](LICENSE.md).

Note that additional terms may apply to the built output.
In particular,
the [softdevice's license] terms become part of what governs the use of the resulting program.

[softdevice's license]: https://www.nordicsemi.com/Products/Development-software/s132/download
