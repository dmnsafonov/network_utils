#! /bin/bash

if [[ "$1" == "setup" ]]; then
    ip link add rsp-inner type veth peer name rsp-mid-in
    ip link add rsp-outer type veth peer name rsp-mid-out

    sysctl -q net.ipv6.conf.rsp-inner.forwarding=0
    sysctl -q net.ipv6.conf.rsp-outer.forwarding=0
    sysctl -q net.ipv6.conf.rsp-mid-in.forwarding=1
    sysctl -q net.ipv6.conf.rsp-mid-out.forwarding=1

    ip netns add rsp-inner
    ip netns add rsp-middle
    ip netns add rsp-outer

    ip link set rsp-inner netns rsp-inner
    ip link set rsp-mid-in netns rsp-middle
    ip link set rsp-mid-out netns rsp-middle
    ip link set rsp-outer netns rsp-outer

    ip netns exec rsp-inner ip link set lo up
    ip netns exec rsp-middle ip link set lo up
    ip netns exec rsp-outer ip link set lo up

    ip netns exec rsp-inner ip link set rsp-inner up
    ip netns exec rsp-middle ip link set rsp-mid-in up
    ip netns exec rsp-middle ip link set rsp-mid-out up
    ip netns exec rsp-outer ip link set rsp-outer up

    ip netns exec rsp-middle ip addr add fc00::1:ffff/104 dev rsp-mid-in
    ip netns exec rsp-middle ip addr add fc00::2:ffff/64 dev rsp-mid-out

    ip netns exec rsp-inner ip addr add fc00::1:1/64 dev rsp-inner
    ip netns exec rsp-outer ip addr add fc00::2:1/64 dev rsp-outer
elif [[ "$1" == "clean" ]]; then
    ip netns del rsp-inner
    ip netns del rsp-middle
    ip netns del rsp-outer
elif [[ "$1" == "innershell" ]]; then
    ip netns exec inner bash
elif [[ "$1" == "middleshell" ]]; then
    ip netns exec middle bash
elif [[ "$1" == "outershell" ]]; then
    ip netns exec outer bash
elif [[ "$1" == "kill" ]]; then
    killall -9 rsndpproxy
else
    echo 'use setup, clean, innershell, middleshell, or outershell'
fi
