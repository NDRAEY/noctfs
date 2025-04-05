[org 0x7c00]

jmp short 0x3d
nop

times 0x3a db 0

mov si, .message

print:
  lodsb
  
  cmp al, 0
  je .end

  mov ah, 0x0e
  int 0x10
  jmp print

.end:
  jmp $

.message: db "This disk is not bootable. NoctFS v1.0", 0

times 510 - ($-$$) db 0
dw 0xaa55
