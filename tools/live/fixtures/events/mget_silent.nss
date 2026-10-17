// Queue native speech so chat callbacks execute outside the calling chunk VM.
void main()
{
    ActionSpeakString("silent-talk-proof", TALKVOLUME_SILENT_TALK);
    ActionSpeakString("silent-shout-proof", TALKVOLUME_SILENT_SHOUT);
    ActionSpeakString("audible-proof", TALKVOLUME_TALK);
}
