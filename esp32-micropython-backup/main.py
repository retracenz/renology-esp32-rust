from machine import Pin, ADC, SPI
import sh1106
import time
import esp32

# Hardware SPI Setup
sck = Pin(18)   # SCK
mosi = Pin(23)  # MOSI
cs = Pin(5)     # Chip Select
a0 = Pin(4)     # A0 (Data/Command)
rst = Pin(2)    # Reset (Optional)

spi = SPI(1, baudrate=4000000, polarity=0, phase=0, sck=sck, mosi=mosi)

# Initialize OLED Display
oled = sh1106.SH1106_SPI(128, 64, spi, a0, rst, cs)

# ADC Setup (ESP32 has 12-bit ADC, values 0-4095)
adc = ADC(Pin(35))
adc.atten(ADC.ATTN_11DB)  # Allows full 0-3.3V range

# ADC Setup (ESP32 has 12-bit ADC, values 0-4095)
adc2 = ADC(Pin(34))
adc2.atten(ADC.ATTN_11DB)  # Allows full 0-3.3V range

# Buffer for Oscilloscope Trace (128 pixels wide)
trace = [0] * 80
trace2 = [0] * 80

# Voltage divider factor (10k / 47k divider)
divider_ratio = (47 + 10) / 10 * 1.09  # = 5.7

# # Linear interpolation parameters
# v1, scale1 = 2.06, (12.85/2.06)   # At 3.3V, correction = 1.0 (no change)
# v2, scale2 = 0.13, (1.5/0.13)  # At 0.23V, correction = 1.78

# # Calculate linear scale equation: Scale = m * V_raw + b
# m = (scale2 - scale1) / (v2 - v1)  # Slope
# b = scale1 - m * v1  # Intercept

def adc_corrected(adc_value):
    raw_voltage = (adc_value / 4095) * 3.3  # Convert ADC to voltage before correction
    
    return raw_voltage

# Main Loop: Draw Oscilloscope Trace with Voltage Overlay
while True:
    ## Battery 1
    # Read ADC value and convert to voltage (0 - 3.3V)
    adc_value = adc_corrected(adc.read())
    voltage = adc_value * divider_ratio
    y_value =  31 - min(int(voltage)*2,30) #31 - int((adc_value/2))  # Map to screen pixels (top 32 lines)

    # Shift the trace left
    trace.pop(0)
    trace.append(y_value)
    #print(y_value)

    # Clear the top 32 lines for the trace
    oled.fill_rect(0, 0, 80, 31, 0)

    # Draw the oscilloscope trace
    for x in range(80):
        oled.pixel(x, trace[x], 1)

    # Convert voltage to string with 2 decimal places
    voltage_text = "{:.2f}V".format(voltage)

    # Clear space for voltage text
    oled.fill_rect(81, 0, 47, 32, 0)  # Erase previous text
    oled.text(voltage_text, 81, 0, 1)  # Draw voltage at (100, 32)
    oled.text("Batt 1", 81, 10, 1)  # Draw voltage at (100, 32)

    ## Battery 2
    # Read ADC value and convert to voltage (0 - 3.3V)
    adc_value2 = adc_corrected(adc2.read())
    voltage2 = adc_value2* divider_ratio
    #* 5.69
    y_value2 =  62 - min(int(voltage2)*2,30) #60 #62 - int((adc_value2/2))  # Map to screen pixels (bot 32 lines)

    #print(y_value2)

    # Shift the trace left
    trace2.pop(0)
    trace2.append(y_value2)

    # Clear the top 32 lines for the trace
    oled.fill_rect(0, 32, 80, 32, 0)

    # Draw the oscilloscope trace
    for x in range(80):
        oled.pixel(x, trace2[x], 1)
    
    # Convert voltage to string with 2 decimal places
    voltage_text2 = "{:.2f}V".format(voltage2)

    # Clear space for voltage text
    oled.fill_rect(81, 31, 47, 32, 0)  # Erase previous text
    oled.text(voltage_text2, 81, 31, 1)  # Draw voltage at (100, 32)
    oled.text("Batt 2", 81, 41, 1)  # Draw voltage at (100, 32)

    # print(voltage)
    # print(voltage2)

### done populating screen

    # Show updated display
    oled.show()
    
    # Small delay to control refresh rate
    time.sleep(0.05)  # Adjust for speed

