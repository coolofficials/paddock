"use strict";
// Feature tabs are a small, keyboard-operable guide; the product demo remains visible.
const tabs=Array.from(document.querySelectorAll('[role="tab"]'));
function selectTab(tab,focus=false){
  for(const item of tabs){const selected=item===tab;item.setAttribute('aria-selected',String(selected));item.tabIndex=selected?0:-1;document.getElementById(item.getAttribute('aria-controls')).hidden=!selected;}
  if(focus)tab.focus();
}
for(const tab of tabs){
  tab.addEventListener('click',()=>selectTab(tab));
  tab.addEventListener('keydown',event=>{let index=tabs.indexOf(tab);if(event.key==='ArrowRight')index=(index+1)%tabs.length;else if(event.key==='ArrowLeft')index=(index+tabs.length-1)%tabs.length;else if(event.key==='Home')index=0;else if(event.key==='End')index=tabs.length-1;else return;event.preventDefault();selectTab(tabs[index],true);});
}
// Pause the other demonstration when a visitor starts a video.
const videos=Array.from(document.querySelectorAll('video'));
for(const video of videos)video.addEventListener('play',()=>{for(const other of videos)if(other!==video)other.pause();});
