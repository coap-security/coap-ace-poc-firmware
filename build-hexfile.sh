#!/bin/sh

set -e

cargo +nightly build --release --target-dir=target
OURTMP=$(mktemp --directory)
objcopy -O ihex target/thumbv7em-none-eabihf/release/coap-ace-poc-firmware "$OURTMP"/firmware.hex
# Note that we can distribute the result pretty easily -- it's a binary that is
# a software update for a Nordic device, and as such doesn't even need the
# license to be shipped with it.
wget https://nsscprodmedia.blob.core.windows.net/prod/software-and-other-downloads/softdevices/s132/s132_nrf52_7.3.0.zip -O "$OURTMP"/s132_nrf52_7.3.0.zip
unzip "$OURTMP"/s132_nrf52_7.3.0.zip s132_nrf52_7.3.0_softdevice.hex -d "$OURTMP"
# Observations regarding what needs to be in here:
# * If a record 05 (Start Linear Address) is present, it refuses to process the
#   file. (It leaves the file visible and does not eject the block device).
# * If no record 03 (Start Segment Address) is present, it does flash the
#   program, but it runs into some softdevice related panic during execution
#   (observable by starting openocd after the eject, and a gdb with the own
#   binary to interpret the current instruction). Reaching this would be easy
#   to automate using the `-disable Execution_Start_Address` option, but it's
#   bad user experience to need to restart the device.
# * Trying to attach recover details from the error using `probe-run --chip
#   nRF52832_xxAA target/thumbv7em-none-eabihf/release/coap-ace-poc-firmware
#   --no-flash` leads to a reset, after which things just work (but error
#   information is lost).
# * If the record 03 is present with the value of the produced hex file, the
#   same error occurs.
# * In both error situation, both a `montor reset halt` / `c` or re-powering
#   the board resolves the issue.
# * The hex files provided by Nordic have no start address. (eg.
#   proximity_demo/ble_app_proximity_s132_pca10040.hex,
#   heart_rate_demo/heart_rate_demo.hex).
srec_cat -disable Execution_Start_Address ./uicr_reset_pin21.hex -Intel "$OURTMP"/firmware.hex -Intel "$OURTMP"/s132_nrf52_7.3.0_softdevice.hex -Intel -o coap-ace-poc-firmware.hex -Intel -Output_Block_Size 16
rm -rf "${OURTMP}"
