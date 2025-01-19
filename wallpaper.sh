if [ $# -gt 1 ]
then
	echo "Usage: $0 [PATH]"
fi

if [ $# -eq 1 ]
then
	echo $1 > /tmp/cycle-wallpaper.next
fi

pkill cycle-wallpaper -10
