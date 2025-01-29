/* SPDX-FileCopyrightText: Copyright 2022-2024 EDF (Électricité de France S.A.)
 * SPDX-License-Identifier: BSD-3-Clause
 * See README for all details on copyright, authorship and license.
 */
MEMORY
{
  /* These values correspond to the NRF52832_xxAA with SoftDevices S152 7.3.0 */
  FLASH : ORIGIN = 0x00000000 + 152K, LENGTH = 512K - 152K
  /* The 27K are arbitrary -- if it's too small, the softdevice will complain
   * at startup; if it's too large, the linker will complain about insufficient
   * RAM. The room needed by the softdevice depends on its initialization
   * parameters. */
  RAM : ORIGIN = 0x20000000 + 27K, LENGTH = 64K - 27K
}
