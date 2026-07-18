/* mps2-an385: 4M ZBT SSRAM1 as flash at 0x0, 4M SSRAM2/3 as RAM. The
   LM3S6965's 256K flash stopped fitting once the XUASTC tables joined the
   transcoder. */
MEMORY
{
  FLASH : ORIGIN = 0x00000000, LENGTH = 4M
  RAM   : ORIGIN = 0x20000000, LENGTH = 4M
}
