It’s slow because you’re pushing ~110 KB per frame over a 12 MHz SPI shared with SD. That caps you around ~13 fps best case (12 Mbit/s ≈ 1.5 MB/s; 1.5 MB/s ÷ 110 KB ≈ 13.6 fps) before any overhead. Options, in order of impact:
1.	Crank LCD SPI clock (biggest win)

	•	Use 40–60–80 MHz for the LCD phase. Keep SD at ≤20 MHz.
	•	Reconfigure the same SPI between phases:
	•	during SD init: 400 kHz
	•	SD read: ~16–20 MHz
	•	LCD blit: 40–80 MHz
	•	In esp-hal, do spi.change_config(spi_cfg.with_frequency(Rate::from_mhz(40))) before blitting, then switch back before the next SD read.


	2.	Stop per-pixel overhead; blast raw pixels

	•	Avoid embedded-graphics draw loops for full frames. Send one set_address_window(0,0,171,319) then write the entire frame buffer in one MemoryWrite.
	•	If your mipidsi API exposes a raw write (address window + write), use that. It cuts CPU overhead drastically.

	3.	Use DMA + ping-pong buffering

	•	Enable SPI GDMA (C6 supports it). Let DMA stream the last line/frame to the LCD while the CPU reads the next chunk from SD.
	•	Two buffers (“A” and “B”): while DMA sends A, fill B from SD; swap.
	•	Minimum chunk per kick: at least a few KB; 16–32 KB is healthy.

	4.	Stream line-by-line (or tile) instead of full frame

	•	Current frame = 110,080 B. If SRAM is tight, allocate two line buffers (e.g., 172×2×2 = 688 B each) or small tiles (e.g., 172×32×2 = 11 KB), and pipeline read→blit.

	5.	Remove artificial delays & pace by bandwidth

	•	You have delay.delay_millis(3) (or 33 previously). That hard-caps fps. Remove it or make it conditional (only for demo pacing).

	6.	Use larger, aligned SD reads

	•	Read in multiples of 512 B and preferably 4–16 KB chunks to reduce SD command overhead.
	•	Keep CS asserted while reading sequential sectors.

	7.	Separate buses (if you can)

	•	Ideal: SD on SPI2, LCD on SPI3 (or LCD peripheral on S3; C6 has fewer, but if you have free SPI, use it). Then both can run at their optimal clocks without reconfig churn.

		1.a	Put LCD on a separate SPI (best win)

		•	Keep SD on SPI2 @ 20–25 MHz (SPI mode).
		•	Move LCD to SPI3 @ 60–80 MHz (try 60 first; many ST7789s spec ~62.5 MHz, 80 often works).
		•	Code: create a second Spi with its own SpiConfig, own CS/DC pins, and build the display on that bus. No runtime re-clocking; each bus stays fixed.
		•	Expect: 2–4× faster blits and less tearing.

	8.	Lower the pixels you push

	•	Pre-resize: 160×288 (FFmpeg scale=160:288) → 92 KB/frame → ~16% less data.
	•	Crop letterbox if you don’t need the whole display.
	•	If acceptable, drop fps in source (duplicate frames) to reduce SD load.

	9.	Overclock CPU (smaller win)

	•	You’re already on CpuClock::max(). Gains are limited vs. raising SPI.

If you want, I can patch:
•	add spi.change_config(...) to switch 20 MHz (SD) ⇄ 40 MHz (LCD),
•	remove the per-frame delay,
•	convert the blit path to address-window + single raw write,
•	and switch to tile ping-pong (e.g., 172×32).

Say “generate edits” and I’ll wire those in your main.rs.