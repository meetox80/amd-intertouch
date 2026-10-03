ifneq ($(KERNELRELEASE),)
obj-m += drivers/i2c/busses/
obj-m += drivers/input/rmi4/
obj-m += drivers/input/mouse/
else
KDIR ?= /usr/lib/modules/$(shell uname -r)/build
all:
	$(MAKE) -C $(KDIR) M=$(CURDIR) modules
clean:
	$(MAKE) -C $(KDIR) M=$(CURDIR) clean
endif
