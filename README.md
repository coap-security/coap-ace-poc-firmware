<!--
SPDX-FileCopyrightText: Copyright 2022 EDF (Électricité de France S.A.)
SPDX-License-Identifier: BSD-3-Clause
-->
CoAP/ACE-OAuth PoC: Firmware
============================

This repository contains the firmware part of the CoAP/ACE-OAuth proof-of-concept implementation.
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

* an [ACE-OAuth (RFC9200)] Resource Server (RS) -- this limits the interactions of users according to an authorization server's decisions
* [OSCORE (RFC8613)] -- this secures communication with symmetric keys, independent of the precise transport mechanism used
* [EDHOC (RFC9528)] -- this establishes fresh symmetric key material from asymmetric keys with forward secrecy
* at runtime, any of
  * the [ACE OSCORE profile (RFC9203)] -- this connects the ACE framework with OSCORE, and generates OSCORE keys
  * the [ACE EDHOC profile] -- this connects the ACE framework with EDHOC
* [CoAP (RFC7252)] -- this gives a compact and versatile application protocol with flexible forwarding options
* [CoAP-over-GATT (`draft-amsuess-core-coap-over-gatt-02`)] -- this allows transporting CoAP over Bluetooth Low Energy (BLE) without the need to set up a Bluetooth IP network

[ACE (RFC9200)]: https://www.rfc-editor.org/rfc/rfc9200.html
[ACE OSCORE profile (RFC9203)]: https://www.rfc-editor.org/rfc/rfc9203.html
[ACE EDHOC profile]: https://datatracker.ietf.org/doc/draft-ietf-ace-edhoc-oscore-profile/
[OSCORE (RFC8613)]: https://www.rfc-editor.org/rfc/rfc8613.html
[CoAP (RFC7252)]: https://www.rfc-editor.org/rfc/rfc7252.html
[EDHOC (RFC9528)]: https://datatracker.ietf.org/doc/html/rfc9528
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
  Press "Search nearby devices" and pick the device that is shown.
* As the device requires authentication,
  follow the "Login" link, and use the name *technician* (password: *technician*) or *junior* (password: *junior*).
* Use the web application's controls to read the device's temperature,
  to find the device (making its LEDs spin briefly),
  or to alter its identification LEDs (not available to *junior*).

  For illustration purposes, the web application is not made aware of the permission levels,
  and unauthorized control attempts will fail.
* You may also install the mobile application through the browser's "Install app" button.

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

* Backing the authentication is a keycloak server running the [ACE OAuth Extension].
  A public instance is available on `https://keycloak.coap.amsuess.com/`;
  its setup resembles the "playground" configuration that is [provided for development situations].


[firmware's documentation]: https://oscore.gitlab.io/coap-ace-poc-firmware/doc/coap_ace_poc_firmware/
[documented there as well]: https://oscore.gitlab.io/coap-ace-poc-webapp/doc/coap_ace_poc_webapp/
[ACE OAuth Extension]: https://gitlab.com/oscore/keycloak-ace-oauth-extension/
[provided for development situations]: https://gitlab.com/oscore/keycloak-ace-oauth-extension/-/tree/main/playground

License
-------

This project and all files contained in it is published under the
BSD-3-Clause license as defined in [`LICENSES/BSD-3-Clause.txt`](LICENSES/BSD-3-Clause.txt).

Copyright: 2022-2024 EDF (Électricité de France S.A.)

Author: Christian Amsüss

Note that additional terms may apply to the built output.
In particular,
the [softdevice's license] terms become part of what governs the use of the resulting program.

[softdevice's license]: https://www.nordicsemi.com/Products/Development-software/s132/download
