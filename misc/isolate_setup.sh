# DO NOT EXECUTE THIS SCRIPT DIRECTLY!

exit 1

# ====================  run below as ROOT  ==========================

# --------------- set up isolate ---------------

# Test if cgroup v2 is available. Isolate requires cgroup v2.
[ -f /sys/fs/cgroup/cgroup.controllers ] && echo "cgroup v2 (unified) present" || echo "no cgroup v2"

# Make sure you have:
# -  git, make, gcc, g++
# -  pkg-config
# -  headers for the libcap library (usually available in a libcap-dev package)
# -  headers for the libsystemd library (libsystemd-dev package)

cd /root && git clone https://github.com/ioi/isolate.git --depth 1

cd isolate && make isolate && make install

# Read the output. The sandbox directory may be "/var/local/lib/isolate/".
# Once initialized, the sandbox with ID 0 gets chroot "/var/local/lib/isolate/0/", 
# whose owner is the user who runs `isolate`.
# Inside is a minimal Linux environment.
# Its home and start location is "/var/local/lib/isolate/0/box/".

systemctl daemon-reload
systemctl enable --now isolate.service

# Check status
systemctl status isolate.service

# Check environment and MANUALLY do the tweaks it needs
isolate-check-environment

# --------------- set up rustup ---------------

# (optional)
export RUSTUP_DIST_SERVER="https://rsproxy.cn"
export RUSTUP_UPDATE_ROOT="https://rsproxy.cn/rustup"

curl --proto '=https' --tlsv1.2 -sSf https://rsproxy.cn/rustup-init.sh | sh

mkdir -p /opt/oj/rust

export CARGO_HOME=/opt/oj/rust/cargo
export RUSTUP_HOME=/opt/oj/rust/rustup

cat > $CARGO_HOME/config.toml << 'EOF'
[source.crates-io]
replace-with = 'rsproxy-sparse'
[source.rsproxy]
registry = "https://rsproxy.cn/crates.io-index"
[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"
[registries.rsproxy]
index = "https://rsproxy.cn/crates.io-index"
[net]
git-fetch-with-cli = true
[build]
# Set max parallel jobs
jobs = 4
EOF

chmod -R 755 /opt/oj

# ====================  below is a test, run as a NON-ROOT user ==========================

isolate -b 3 --cg --init

# Prepare `code.rs`, `code.cpp`, `code.c` as test code
cp code.* /var/local/lib/isolate/3/box/

alias compile="isolate -b 3 --cg --run --processes=10 --open-files=512 --fsize=65536 --wall-time=30 --cg-mem=262144 --dir=/opt/oj --dir=/etc/alternatives -E RUSTUP_HOME=/opt/oj/rust/rustup -E CARGO_HOME=/opt/oj/rust/cargo -E PATH=/opt/oj/rust/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin --stderr-to-stdout -o compile.out -M /tmp/box3.meta --"

# Note: 
# the meta file location is relative to the host machine, not the sandbox!
# the stdout file location is inside its /box
compile /bin/sh -c 'rustc -o main-rust code.rs'
compile /bin/sh -c 'g++ -o main-cpp code.cpp'
compile /bin/sh -c 'gcc -o main-c code.c -lm'

cat /tmp/3.meta
# A SUCCESSFUL COMPILE:
# time:0.306
# time-wall:0.308
# max-rss:96128
# csw-voluntary:55
# csw-forced:10
# cg-mem:63244
# exitcode:0

alias run="isolate -b 3 --cg --run --processes=4 --open-files=30 --fsize=16384 --time=1 --wall-time=5 --extra-time=1 --cg-mem=131072 --stack=65536 --dir=/opt/oj --dir=/etc/alternatives -E RUSTUP_HOME=/opt/oj/rust/rustup -E CARGO_HOME=/opt/oj/rust/cargo -E PATH=/opt/oj/rust/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin --stderr-to-stdout -o case.out -M /tmp/box3.meta --"

cat /tmp/3.meta
# (RUN PANICKED)
# time:0.003
# time-wall:0.013
# max-rss:2048
# csw-voluntary:8
# csw-forced:2
# cg-mem:63268
# exitcode:101
# status:RE
# message:Exited with error status 101

# (CPU TIME TIMEOUT)
# status:TO
# message:Time limit exceeded
# killed:1
# time:1.086
# time-wall:1.100
# max-rss:640
# csw-voluntary:4
# csw-forced:0
# cg-mem:63360

# (WALL TIMEOUT)
# status:TO
# message:Time limit exceeded (wall clock)
# killed:1
# time:0.003
# time-wall:5.100
# max-rss:640
# csw-voluntary:4
# csw-forced:0
# cg-mem:63360

# (HEAP OUT OF MEMORY)
# time:0.271
# time-wall:0.284
# max-rss:201984
# csw-voluntary:9
# csw-forced:78
# cg-mem:200000
# cg-oom-killed:1
# exitcode:137  (SEEMS FIXED FOR RUST)
# status:RE
# message:Exited with error status 137

# (STACK OVERFLOW)
# time:0.055
# time-wall:0.064
# max-rss:51840
# csw-voluntary:7
# csw-forced:4
# cg-mem:50528
# exitcode:134  (SEEMS FIXED FOR RUST)
# status:RE
# message:Exited with error status 134
#
# (STDOUT + STDERR)
# thread 'main' has overflowed its stack
# fatal runtime error: stack overflow, aborting
# Aborted

# (TRYING TO WRITE SUPER BIG FILE)
# time:0.030
# time-wall:0.042
# max-rss:1920
# csw-voluntary:8
# csw-forced:4
# cg-mem:17504
# exitcode:153  (SEEMS FIXED FOR RUST)
# status:RE
# message:Exited with error status 153

# (TRYING TO FORK MANY PROCESSES) it appears the same as a normal exit
# time:0.006
# time-wall:3.023
# max-rss:2176
# csw-voluntary:12
# csw-forced:1
# cg-mem:1596
# exitcode:0
#
# (STDOUT + STDERR)
# 创建子进程失败: Resource temporarily unavailable (os error 11)
# 创建子进程失败: Resource temporarily unavailable (os error 11)

# (TRYING TO OPEN MANY FILES)
# time:0.003
# time-wall:1.011
# max-rss:2176
# csw-voluntary:9
# csw-forced:2
# cg-mem:768
# exitcode:0
#
# (STDOUT + STDERR)
# 限制 30 个文件, 打开第 28 个文件时失败: Too many open files (os error 24)

isolate -b 3 --cg --cleanup
# Should cleanup on each id. `isolate --cg --cleanup` only applies to 0.