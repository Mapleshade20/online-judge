# isolate meta files

```
(SUCCESSFUL)
time:0.306
time-wall:0.308
max-rss:96128
csw-voluntary:55
csw-forced:10
cg-mem:63244
exitcode:0

(RUN PANICKED)
time:0.003
time-wall:0.013
max-rss:2048
csw-voluntary:8
csw-forced:2
cg-mem:63268
exitcode:101
status:RE
message:Exited with error status 101

(CPU TIME TIMEOUT)
status:TO
message:Time limit exceeded
killed:1
time:1.086
time-wall:1.100
max-rss:640
csw-voluntary:4
csw-forced:0
cg-mem:63360

(WALL TIMEOUT)
status:TO
message:Time limit exceeded (wall clock)
killed:1
time:0.003
time-wall:5.100
max-rss:640
csw-voluntary:4
csw-forced:0
cg-mem:63360

(HEAP OUT OF MEMORY)
time:0.271
time-wall:0.284
max-rss:201984
csw-voluntary:9
csw-forced:78
cg-mem:200000
cg-oom-killed:1
exitcode:137  (SEEMS FIXED FOR RUST)
status:RE
message:Exited with error status 137

(STACK OVERFLOW)
time:0.055
time-wall:0.064
max-rss:51840
csw-voluntary:7
csw-forced:4
cg-mem:50528
exitcode:134  (SEEMS FIXED FOR RUST)
status:RE
message:Exited with error status 134
(STDOUT + STDERR)
thread 'main' has overflowed its stack
fatal runtime error: stack overflow, aborting
Aborted

(TRYING TO WRITE SUPER BIG FILE)
time:0.030
time-wall:0.042
max-rss:1920
csw-voluntary:8
csw-forced:4
cg-mem:17504
exitcode:153  (SEEMS FIXED FOR RUST)
status:RE
message:Exited with error status 153

(TRYING TO FORK MANY PROCESSES) it appears the same as a normal exit
time:0.006
time-wall:3.023
max-rss:2176
csw-voluntary:12
csw-forced:1
cg-mem:1596
exitcode:0
(STDOUT + STDERR)
创建子进程失败: Resource temporarily unavailable (os error 11)
创建子进程失败: Resource temporarily unavailable (os error 11)

(TRYING TO OPEN MANY FILES)
time:0.003
time-wall:1.011
max-rss:2176
csw-voluntary:9
csw-forced:2
cg-mem:768
exitcode:0
(STDOUT + STDERR)
限制 30 个文件, 打开第 28 个文件时失败: Too many open files (os error 24)
```