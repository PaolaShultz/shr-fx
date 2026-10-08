/* Device-free owner C consumer. Compile with C11 -Wall -Wextra -Werror. */
#include "shr_fx.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
_Static_assert(sizeof(shr_fx_channel_v2)==40,"channel size");
_Static_assert(offsetof(shr_fx_channel_v2,bypass)==32,"bypass offset");
_Static_assert(sizeof(shr_fx_config_v2)==104,"config size");
_Static_assert(offsetof(shr_fx_config_v2,channel)==24,"channel offset");
_Static_assert(sizeof(shr_fx_capabilities_v2)==128,"capabilities size");
_Static_assert(offsetof(shr_fx_capabilities_v2,max_delay_storage_bytes)==120,"memory offset");
_Static_assert(sizeof(shr_fx_status_v2)==168,"status size");
_Static_assert(offsetof(shr_fx_status_v2,applied_generation)==32,"generation offset");
_Static_assert(offsetof(shr_fx_status_v2,target)==88,"target offset");
static shr_fx_status_v2 status(void *h) {
 shr_fx_status_v2 s={0}; assert(shr_fx_v2_status(h,&s,2,sizeof s)==0); return s;
}
static uint64_t bits(double x) {uint64_t u;memcpy(&u,&x,sizeof u);return u;}
int main(void) {
 shr_fx_capabilities_v2 caps={0};
 assert(shr_fx_v2_capabilities(&caps,2,sizeof caps)==0);
 assert(strcmp(caps.identity,"fx-a/prepared-delay-v2")==0);
 assert(caps.rack_available==0 && caps.max_read_heads_per_channel==2);
 printf("{\"event\":\"caps\",\"bytes\":%llu,\"channels\":%u}\n",(unsigned long long)caps.max_delay_storage_bytes,caps.channels);
 void *h=shr_fx_v2_create(48000,8192);assert(h);
 shr_fx_status_v2 s=status(h);
 shr_fx_config_v2 config={.version=2,.size=sizeof config,.expected_generation=0,.generation=1};
 config.channel[0]=s.target[0];config.channel[1]=s.target[1];
 config.channel[0].delay_ms=1;config.channel[0].feedback=0;config.channel[0].damping=0;config.channel[0].wet_gain=1;
 void *p=shr_fx_v2_prepare(&config,48000,2,sizeof config);assert(p);
 assert(shr_fx_v2_commit(h,p,0)==0);shr_fx_v2_retire(p);
 s=status(h);assert(s.applied_generation==1 && s.settled_generation==0 && s.remaining_frames[0]==960 && s.remaining_frames[1]==0);
 double input[16384]={0},output[16384]={0};
 assert(shr_fx_v2_process(h,input,output,960,0)==0);
 s=status(h);assert(s.settled_generation==1 && s.settled_source_frame==960);
 input[0]=.5+0x1p-40;input[1]=-.25-0x1p-42;
 assert(shr_fx_v2_process(h,input,output,2000,960)==0);
 assert(output[96]==input[0] && output[1921]==input[1]*.5);
 assert(output[0]==0 && output[1]==0);
 printf("{\"event\":\"impulse\",\"left_onset\":48,\"left_bits\":\"%016llx\",\"right_onset\":960,\"right_bits\":\"%016llx\"}\n",(unsigned long long)bits(output[96]),(unsigned long long)bits(output[1921]));
 config.expected_generation=1;config.generation=2;config.channel[0].bypass=1;
 p=shr_fx_v2_prepare(&config,48000,2,sizeof config);assert(p);
 assert(shr_fx_v2_commit(h,p,2960)==0);shr_fx_v2_retire(p);
 assert(shr_fx_v2_process(h,input,output,1,2960)==0);
 config.expected_generation=2;config.generation=3;config.channel[0].wet_gain=.25;
 p=shr_fx_v2_prepare(&config,48000,2,sizeof config);assert(p);
 int busy=shr_fx_v2_commit(h,p,2961);assert(busy==-5);
 int timeline=shr_fx_v2_process(h,input,output,1,999);assert(timeline==-6);
 assert(shr_fx_v2_panic(h,1)==0);
 memset(input,0,sizeof input);
 assert(shr_fx_v2_process(h,input,output,2000,2961)==0);
 for(unsigned i=0;i<2000;++i)assert(output[i*2]==0);
 assert(shr_fx_v2_commit(h,p,4961)==0);shr_fx_v2_retire(p);
 assert(shr_fx_v2_reset(h,7000)==0);
 s=status(h);assert(s.applied_generation==3 && s.settled_generation==3 && s.next_source_frame==7000);
 config.expected_generation=1;config.generation=4;
 p=shr_fx_v2_prepare(&config,48000,2,sizeof config);assert(p);
 int stale=shr_fx_v2_commit(h,p,7000);assert(stale==-4);shr_fx_v2_retire(p);
 input[0]=NAN;int sample=shr_fx_v2_process(h,input,output,1,7000);assert(sample==-3 && output[0]==0 && output[1]==0);
 s=status(h);assert(s.reset_reason==4 && s.reset_count==3);
 printf("{\"event\":\"errors_status\",\"busy\":%d,\"timeline\":%d,\"stale\":%d,\"sample\":%d,\"applied\":%llu,\"settled\":%llu,\"next_frame\":%llu,\"resets\":%llu}\n",busy,timeline,stale,sample,(unsigned long long)s.applied_generation,(unsigned long long)s.settled_generation,(unsigned long long)s.next_source_frame,(unsigned long long)s.reset_count);
 assert(shr_fx_v2_reset(h,UINT64_MAX-959)==0);
 config.expected_generation=3;config.generation=4;config.channel[0].wet_gain=1;
 p=shr_fx_v2_prepare(&config,48000,2,sizeof config);assert(p);
 int overflow=shr_fx_v2_commit(h,p,UINT64_MAX-959);assert(overflow==-6);shr_fx_v2_retire(p);
 s=status(h);assert(s.applied_generation==3 && s.next_source_frame==UINT64_MAX-959);
 config.channel[0]=s.target[0];config.channel[1]=s.target[1];
 p=shr_fx_v2_prepare(&config,48000,2,sizeof config);assert(p);
 assert(shr_fx_v2_commit(h,p,UINT64_MAX-959)==0);shr_fx_v2_retire(p);
 s=status(h);assert(s.settled_generation==4 && s.settled_source_frame==UINT64_MAX-959);
 printf("{\"event\":\"transition_overflow\",\"changed_result\":%d,\"no_change_settled\":%llu}\n",overflow,(unsigned long long)s.settled_generation);
 shr_fx_v2_destroy(h);
 return 0;
}
