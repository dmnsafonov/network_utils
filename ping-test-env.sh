#! /bin/bash

if [[ "$1" == "setup" ]]; then
    ip link add send-if type veth peer name recv-if

    sysctl -q net.ipv6.conf.send-if.forwarding=0
    ip netns add pingdata-sender
    ip link set send-if netns pingdata-sender
    ip netns exec pingdata-sender ip link set lo up
    ip netns exec pingdata-sender ip link set send-if up
    ip netns exec pingdata-sender ip link set send-if mtu 1280
    ip netns exec pingdata-sender ip addr add fc00::1/64 dev send-if

    sysctl -q net.ipv6.conf.recv-if.forwarding=0
    ip netns add pingdata-receiver
    ip link set recv-if netns pingdata-receiver
    ip netns exec pingdata-receiver ip link set lo up
    ip netns exec pingdata-receiver ip link set recv-if up
    ip netns exec pingdata-receiver ip link set recv-if mtu 1280
    ip netns exec pingdata-receiver ip addr add fc00::2/64 dev recv-if
elif [[ "$1" == "clean" ]]; then
    ip netns del pingdata-sender
    ip netns del pingdata-receiver
elif [[ "$1" == "send" ]]; then
    user="$2"
    shift 2
    ip netns exec pingdata-sender su "$user" -c "$*"
elif [[ "$1" == "recv" ]]; then
    user="$2"
    shift 2
    ip netns exec pingdata-receiver su "$user" -c "$*"
elif [[ "$1" == "permissions" ]]; then
    chown root: target/debug/ping6-datasend
    chown root: target/debug/ping6-datarecv
    setcap 'cap_net_raw=p' target/debug/ping6-datasend
    setcap 'cap_net_raw=p' target/debug/ping6-datarecv
    [ -d target/release ] && chown root: target/release/ping6-datasend
    [ -d target/release ] && chown root: target/release/ping6-datarecv
    [ -d target/release ] && setcap 'cap_net_raw=p' target/release/ping6-datasend
    [ -d target/release ] && setcap 'cap_net_raw=p' target/release/ping6-datarecv
elif [[ "$1" == "kill" ]]; then
    killall -9 ping6-datasend
    killall -9 ping6-datarecv
else
    echo 'use setup, clean, send, recv, permissions, or kill'
fi
